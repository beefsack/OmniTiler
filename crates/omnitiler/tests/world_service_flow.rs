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
    let desktops = match native_ws {
        Some(native) => format!("\"{native}\""),
        None => String::new(),
    };
    let frame = rect
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    assert_eq!(frame.len(), 4, "raw test rects are [x,y,w,h]");
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"frameGeometry\":{{\"x\":{x},\"y\":{y},\"width\":{w},\"height\":{h}}},\
          \"clientGeometry\":{{\"x\":{x},\"y\":{y},\"width\":{w},\"height\":{h}}},\
          \"minSize\":null,\"maxSize\":null,\"desktops\":[{desktops}],\
          \"normalWindow\":true,\"managed\":true,\"deleted\":false,\
          \"transient\":false,\"transientFor\":null,\"dialog\":false,\"modal\":false,\
          \"wantsInput\":true,\"fullScreen\":false,\"maximizeMode\":0,\
          \"onAllDesktops\":false,\"minimized\":false,\"hidden\":false}}",
        x = frame[0],
        y = frame[1],
        w = frame[2],
        h = frame[3],
    )
}

/// Minimal projected inner report: one complete screen, caller-chosen
/// windows, unknown views. Broker-owned sequencing keys are never carried.
/// The automatic birth-clear capability defaults off here (the fail-closed
/// wire default for adapters predating the key); the KDE-route born tests
/// below use [`inner_report_birth`], which carries what the real KWin
/// producer emits.
fn inner_report(windows: &[String], complete: bool, host_owned: bool, native: &str) -> String {
    inner_report_with_attempts(windows, complete, host_owned, native, "", false)
}

/// KDE-producer inner report: like [`inner_report`] but with the automatic
/// birth-clear capability opted in, matching the real KWin producer.
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
          \"hostOwned\":{host_owned},\"equalCount\":false,\"birthClear\":{birth_clear},\"native\":[{native}],\
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
    // A syntactically invalid capture (raw maximizeMode as a string)
    // rejects without touching the engine or the queue.
    let bad_window = window_json("win-001", "[0,0,960,1040]", None)
        .replace("\"maximizeMode\":0", "\"maximizeMode\":\"maybe\"");
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

#[test]
fn raw_capture_rejects_before_drain_without_consuming_counters() {
    // Strict raw lane: missing/malformed/mismatched/extra/duplicate window
    // facts reject before any command drain and before the observation
    // sequence advances, so a queued command stays pending and the next
    // valid capture still mints the unskipped revision.
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
            "malformed",
            mutate("\"maximizeMode\":0", "\"maximizeMode\":\"maybe\""),
            "unknown-value",
        ),
        (
            "mismatched",
            mutate("\"win\":\"win-001\"", "\"win\":\"has space\""),
            "unknown-value",
        ),
        (
            "extra",
            mutate(
                "\"minimized\":false",
                "\"minimized\":false,\"floating\":false",
            ),
            "unknown-field",
        ),
        (
            "legacy-verdict",
            mutate(
                "\"minimized\":false",
                "\"minimized\":false,\"fixedAuto\":false",
            ),
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
    // A remembered raw rejection replays byte-identically (never draining);
    // the same source with different bytes refuses as reused.
    let (service, generation) = discovered_service();
    let bad_inner = inner_report(
        &[window_json("win-001", "[0,0,960,1040]", None)
            .replace("\"maximizeMode\":0", "\"maximizeMode\":\"maybe\"")],
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
fn raw_kwin_shape_projects_fixed_minimized_overlay_and_safety() {
    // Realistic KWin captured shape: native UUID desktop ids, QSizeF width /
    // height sentinels (0x0 minimum, Qt unbounded maxima, i32::MAX), partial
    // bounds, fractional frame/client hints, minimized retention, sticky and
    // fullscreen overlays, a fixed override lane, and a dropped non-normal
    // window. The broker converts frame-space hints mechanically and calls
    // the shared fixed classifier on the captured client sizes.
    let (service, generation) = discovered_service();
    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\"}}"
    );
    let fixed = window_json("win-fixed", "[0,0,408,308]", Some(UUID_A))
        .replace(
            "\"minSize\":null",
            "\"minSize\":{\"width\":400,\"height\":300}",
        )
        .replace(
            "\"maxSize\":null",
            "\"maxSize\":{\"width\":400,\"height\":300}",
        )
        .replace(
            "\"clientGeometry\":{\"x\":0,\"y\":0,\"width\":408,\"height\":308}",
            "\"clientGeometry\":{\"x\":4,\"y\":4,\"width\":400,\"height\":300}",
        );
    let sentinel = window_json("win-sentinel", "[0,0,200,200]", Some(UUID_A))
        .replace("\"minSize\":null", "\"minSize\":{\"width\":0,\"height\":0}")
        .replace(
            "\"maxSize\":null",
            "\"maxSize\":{\"width\":16777215,\"height\":16777215}",
        );
    let unbounded = window_json("win-unbounded", "[0,0,200,200]", Some(UUID_A))
        .replace(
            "\"minSize\":null",
            "\"minSize\":{\"width\":176,\"height\":150}",
        )
        .replace(
            "\"maxSize\":null",
            "\"maxSize\":{\"width\":2147483647,\"height\":2147483647}",
        );
    let minimized = window_json("win-min", "[0,0,200,200]", Some(UUID_B))
        .replace("\"minimized\":false", "\"minimized\":true");
    let overlay = window_json("win-full", "[0,0,1920,1080]", Some(UUID_B))
        .replace("\"fullScreen\":false", "\"fullScreen\":true");
    let dialog = window_json("win-dialog", "[0,0,200,200]", None)
        .replace("\"normalWindow\":true", "\"normalWindow\":false");
    // The fullscreen member rides a non-current desktop (as in the
    // production fixture): the overlay is retained there while the current
    // scope stays controlled.
    let inner = inner_report(
        &[fixed, sentinel, unbounded, minimized, overlay, dialog],
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
    assert_eq!(windows.len(), 5, "non-normal window drops, rest project");
    let row = |id: &str| {
        windows
            .iter()
            .find(|w| w["win"] == id)
            .unwrap_or_else(|| panic!("projected row {id}"))
            .clone()
    };
    // Equal nonzero client pin floats automatically.
    let fixed_row = row("win-fixed");
    assert_eq!(fixed_row["floating"], true);
    // Zero/sentinel hints ride for core-side sanitize without floating.
    assert_eq!(row("win-sentinel")["floating"], false);
    // Ordinary resizeable client with a real minimum and an unbounded
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
    // Overlays ride truthfully on their native home; the game pause lane
    // stays unsupported (no any-fullscreen-game heuristic): the report
    // ingress still carries paused=false with a fullscreen member present.
    let full_row = row("win-full");
    assert_eq!(full_row["ws"], "ws-2");
    let inner_value: serde_json::Value = serde_json::from_str(&inner).expect("inner parses");
    assert_eq!(inner_value["presentation"]["paused"], false);
}

#[test]
fn raw_intent_lanes_float_and_suppress_without_verdicts() {
    // Adapter-retained intent rides dedicated lanes (never per-row
    // verdicts): a floated plain window and a tile-suppressed fixed pin
    // project accordingly, and the lanes never reach the engine.
    let (service, generation) = discovered_service();
    let fixed = window_json("win-fixed", "[0,0,400,300]", None)
        .replace(
            "\"minSize\":null",
            "\"minSize\":{\"width\":400,\"height\":300}",
        )
        .replace(
            "\"maxSize\":null",
            "\"maxSize\":{\"width\":400,\"height\":300}",
        );
    let plain = window_json("win-plain", "[0,0,400,300]", None);
    let mut inner_value: serde_json::Value =
        serde_json::from_str(&inner_report(&[fixed, plain], true, false, ""))
            .expect("inner parses");
    let map = inner_value.as_object_mut().expect("object");
    map.insert(
        "floatIntent".to_owned(),
        serde_json::Value::Array(vec![serde_json::Value::String("win-plain".to_owned())]),
    );
    map.insert(
        "tileIntent".to_owned(),
        serde_json::Value::Array(vec![serde_json::Value::String("win-fixed".to_owned())]),
    );
    let inner = inner_value.to_string();
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

/// N6 born-clear journey through the actual production lane: captured-shaped
/// KWin raw rows (never real capture) -> real Rust `kde_raw` normalizer ->
/// broker stamping/sequencing -> core `WorldEngine` -> `clear_op` desires ->
/// simulated native actuation as raw `clearAttempts` -> core settlement.
/// UUID-like window/desktop ids, fractional frame/client facts, native
/// maximization enums, meaningful/unbounded client hints, and the current
/// context shape of the existing raw corpus helpers above.
const BORN_WIN_A: &str = "550e8400-e29b-41d4-a716-4466554400a1";
const BORN_WIN_B: &str = "550e8400-e29b-41d4-a716-4466554400a2";
const BORN_WIN_C: &str = "550e8400-e29b-41d4-a716-4466554400a3";

/// Captured-shaped KWin raw row with fractional geometry, native overlay
/// enums, and client size hints. Mirrors `kwin/src/world-observe.ts`
/// `RawWindowFact` (20 keys, no projected verdicts).
#[allow(clippy::too_many_arguments)]
fn born_raw(
    win: &str,
    frame: &str,
    client: Option<&str>,
    min_size: Option<&str>,
    max_size: Option<&str>,
    desktop: Option<&str>,
    maximize_mode: i32,
    full_screen: bool,
    minimized: bool,
    normal_window: bool,
    managed: bool,
    deleted: bool,
) -> String {
    let client_json = client.unwrap_or("null");
    let min_json = min_size.unwrap_or("null");
    let max_json = max_size.unwrap_or("null");
    let desktops = desktop.map_or(String::new(), |d| format!("\"{d}\""));
    format!(
        "{{\"win\":\"{win}\",\"out\":\"DP-1\",\
          \"frameGeometry\":{frame},\"clientGeometry\":{client_json},\
          \"minSize\":{min_json},\"maxSize\":{max_json},\"desktops\":[{desktops}],\
          \"normalWindow\":{normal_window},\"managed\":{managed},\"deleted\":{deleted},\
          \"transient\":false,\"transientFor\":null,\"dialog\":false,\"modal\":false,\
          \"wantsInput\":true,\"fullScreen\":{full_screen},\"maximizeMode\":{maximize_mode},\
          \"onAllDesktops\":false,\"minimized\":{minimized},\"hidden\":false}}"
    )
}

fn born_native_order() -> String {
    format!("{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\"}}")
}

/// Inject the tolerant `bornMaximized` ingress mode into a built inner
/// report (absent backfills to tile-on-launch, like older adapters).
fn with_born_mode(inner: &str, mode: &str) -> String {
    inner.replace(
        "\"refill\":\"last-remaining-workspace\",",
        &format!("\"refill\":\"last-remaining-workspace\",\"bornMaximized\":\"{mode}\","),
    )
}

/// Inject a raw `clearAttempts` lane (the production apply-lane record:
/// `{op,win,setter,pre,post,attemptMs}` with a decimal-string clock stamp)
/// into a built inner report.
fn with_clears(inner: &str, attempts: &str) -> String {
    inner.replace(
        "\"views\":[",
        &format!("\"clearAttempts\":[{attempts}],\"views\":["),
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

/// Maximized, non-fixed birth facts: fractional full-work-area frame with a
/// decorated client delta and real minimum/unbounded maximum hints.
fn born_maximized_row(win: &str, desktop: &str) -> String {
    born_raw(
        win,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(desktop),
        3,
        false,
        false,
        true,
        true,
        false,
    )
}

fn born_restored_row(win: &str, desktop: &str) -> String {
    born_raw(
        win,
        "{\"x\":100.4,\"y\":100.6,\"width\":958.5,\"height\":500.5}",
        Some("{\"x\":104.5,\"y\":130.4,\"width\":950.6,\"height\":462.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(desktop),
        0,
        false,
        false,
        true,
        true,
        false,
    )
}

#[test]
fn born_maximized_cohort_clears_and_settles_through_broker() {
    // Same-desktop maximized cohort: first complete captured-shaped report
    // binds both births through the real normalizer/broker/core lane, the
    // reply rides both one-shot `clear_op` tokens with no geometry while
    // held, and correlated accepted attempts plus restored facts settle both
    // with no re-mint.
    let (service, generation) = discovered_service();
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
        .find(|s| s["scope"] == format!("DP-1/{ws}"))
        .unwrap_or_else(|| panic!("cohort scope: {}", first["scopes"]))
        .clone();
    assert_eq!(scope["auth"], "hold", "cohort geometry hold stands");
    // Simulated native actuation: both clears dispatch, then restored facts
    // plus the correlated raw attempts ride the next capture.
    let second_inner = with_clears(
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
        .find(|s| s["scope"] == format!("DP-1/{ws}"))
        .expect("converged scope")
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
        let (service, generation) = discovered_service();
        let native = born_native_order();
        let birth = born_raw(
            BORN_WIN_C,
            "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1079.5}",
            Some("{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1079.5}"),
            None,
            None,
            Some(UUID_A),
            0,
            true,
            false,
            true,
            true,
            false,
        );
        let birth_inner = with_born_mode(&inner_report_birth(&[birth], true, true, &native), mode);
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
        let exit_inner = with_born_mode(
            &inner_report_birth(
                &[born_maximized_row(BORN_WIN_C, UUID_A)],
                true,
                true,
                &native,
            ),
            mode,
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
        let settled_inner = with_clears(
            &with_born_mode(
                &inner_report_birth(
                    &[born_restored_row(BORN_WIN_C, UUID_A)],
                    true,
                    true,
                    &native,
                ),
                mode,
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
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let fixed_birth = born_raw(
        BORN_WIN_A,
        "{\"x\":0.5,\"y\":0.5,\"width\":407.6,\"height\":307.5}",
        Some("{\"x\":4.5,\"y\":4.4,\"width\":399.6,\"height\":299.5}"),
        Some("{\"width\":400,\"height\":300}"),
        Some("{\"width\":400,\"height\":300}"),
        Some(UUID_A),
        3,
        false,
        false,
        true,
        true,
        false,
    );
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
    let (service, generation) = discovered_service();
    let birth = born_raw(
        BORN_WIN_B,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1079.5}",
        Some("{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1079.5}"),
        None,
        None,
        Some(UUID_A),
        0,
        true,
        false,
        true,
        true,
        false,
    );
    service
        .evaluate_capture(&capture(
            ADAPTER,
            &generation,
            1,
            1,
            &inner_report_birth(&[birth], true, true, &native),
        ))
        .expect("accepted");
    let fixed_exit = born_raw(
        BORN_WIN_B,
        "{\"x\":0.5,\"y\":0.5,\"width\":407.6,\"height\":307.5}",
        Some("{\"x\":4.5,\"y\":4.4,\"width\":399.6,\"height\":299.5}"),
        Some("{\"width\":400,\"height\":300}"),
        Some("{\"width\":400,\"height\":300}"),
        Some(UUID_A),
        3,
        false,
        false,
        true,
        true,
        false,
    );
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
    // reclears the latched lifetime.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let kept_inner = with_born_mode(
        &inner_report_birth(
            &[born_maximized_row(BORN_WIN_A, UUID_A)],
            true,
            true,
            &native,
        ),
        "keep-maximized-overlay",
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
    let (service, generation) = discovered_service();
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
    let second_inner = with_clears(
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
    let (service, generation) = discovered_service();
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
    let second_inner = with_clears(
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
fn born_raw_safety_boundary_drops_non_normal_and_holds_minimized() {
    // The existing roster contract at the raw boundary: non-normal windows
    // drop before projection, minimized facts retain with no clear. The
    // `managed`/`deleted` lifecycle lanes map in the normalizer (deleted
    // closes, unmanaged is absent); the dedicated lifecycle tests below
    // pin that boundary.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let dialog = born_raw(
        BORN_WIN_A,
        "{\"x\":100.5,\"y\":100.5,\"width\":400.0,\"height\":300.0}",
        Some("{\"x\":100.5,\"y\":100.5,\"width\":400.0,\"height\":300.0}"),
        None,
        None,
        Some(UUID_A),
        3,
        false,
        false,
        false,
        true,
        false,
    );
    let minimized = born_raw(
        BORN_WIN_B,
        "{\"x\":100.5,\"y\":100.5,\"width\":400.0,\"height\":300.0}",
        Some("{\"x\":100.5,\"y\":100.5,\"width\":400.0,\"height\":300.0}"),
        None,
        None,
        Some(UUID_B),
        3,
        false,
        true,
        true,
        true,
        false,
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(&[dialog, minimized], true, true, &native),
            ))
            .expect("accepted"),
    );
    let windows = reply["windows"].as_array().expect("windows");
    assert!(
        windows.iter().all(|w| w["win"] != BORN_WIN_A),
        "non-normal window drops before projection"
    );
    assert_eq!(
        clear_op_of(&reply, BORN_WIN_B),
        "",
        "minimized proves no birth"
    );
}

#[test]
fn born_raw_deleted_closes_without_candidate_or_clear() {
    // Decided lifecycle boundary (no new core fields): KWin `deleted`
    // (already closed, kept only for compositor animations) means CLOSE.
    // The row never becomes a candidate WindowFact and mints no born
    // clear; the close retires through the existing roster machinery.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let row = born_raw(
        BORN_WIN_C,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        3,
        false,
        false,
        true,
        true,
        true,
    );
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
        "deleted never a candidate: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "deleted");
}

#[test]
fn born_raw_unmanaged_absent_without_close_or_clear() {
    // Decided lifecycle boundary: KWin `managed=false`
    // (application-handled override-redirect: menus, tooltips, DnD icons)
    // means absent. The row never becomes a candidate WindowFact, mints no
    // born clear, and reports no close: unmanaged absence is NOT a
    // membership-policy verdict.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let row = born_raw(
        BORN_WIN_C,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        3,
        false,
        false,
        true,
        false,
        false,
    );
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
        "unmanaged never a candidate: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "unmanaged");
}

#[test]
fn born_raw_deleted_takes_precedence_over_unmanaged() {
    // A row that is both unmanaged and deleted still closes: gone wins
    // over non-manageable.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let row = born_raw(
        BORN_WIN_C,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        3,
        false,
        false,
        true,
        false,
        true,
    );
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
    assert!(
        reply["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| w["win"] != BORN_WIN_C),
        "deleted precedence closes: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "deleted-precedence");
}

#[test]
fn born_raw_deleted_retires_roster_slot_for_re_admission() {
    // An admitted roster member that reports deleted leaves the roster
    // (no candidate, no clear), and the close prunes the born lifetime so
    // a later re-admission evaluates as a fresh lifetime that may clear
    // again.
    let (service, generation) = discovered_service();
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
    let tombstone = born_raw(
        BORN_WIN_A,
        "{\"x\":100.4,\"y\":100.6,\"width\":958.5,\"height\":500.5}",
        Some("{\"x\":104.5,\"y\":130.4,\"width\":950.6,\"height\":462.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        0,
        false,
        false,
        true,
        true,
        true,
    );
    let second = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                2,
                2,
                &inner_report_birth(&[tombstone], true, true, &native),
            ))
            .expect("accepted"),
    );
    assert!(
        second["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| w["win"] != BORN_WIN_A),
        "deleted roster member leaves: {}",
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
fn born_raw_deleted_close_authoritative_on_partial_inventory() {
    // A deleted row closes even when the inventory is partial: the
    // explicit lifecycle fact never holds the scope waiting for geometry.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let row = born_raw(
        BORN_WIN_C,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        3,
        false,
        false,
        true,
        true,
        true,
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(&[row], false, true, &native),
            ))
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
fn born_raw_duplicate_explicit_and_native_close_dedups() {
    // An explicit close plus a native deleted row for the same id is one
    // close: the capture accepts instead of refusing a duplicate.
    let (service, generation) = discovered_service();
    let native = born_native_order();
    let row = born_raw(
        BORN_WIN_C,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        3,
        false,
        false,
        true,
        true,
        true,
    );
    let inner = with_closed(
        &inner_report_birth(&[row], true, true, &native),
        &[BORN_WIN_C],
    );
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
        "deduped close still applies: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "dedup-close");
}

#[test]
fn born_raw_lifecycle_close_keeps_queue_and_replay() {
    // A lifecycle close drains like any fresh capture: the queued command
    // settles, and redelivery replays byte-identically without re-draining.
    let (service, generation) = discovered_service();
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
    let row = born_raw(
        BORN_WIN_C,
        "{\"x\":0.5,\"y\":0.5,\"width\":1919.6,\"height\":1039.5}",
        Some("{\"x\":8.5,\"y\":30.4,\"width\":1903.6,\"height\":1001.5}"),
        Some("{\"width\":200,\"height\":120}"),
        None,
        Some(UUID_A),
        3,
        false,
        false,
        true,
        true,
        true,
    );
    let envelope = capture(
        ADAPTER,
        &generation,
        1,
        1,
        &inner_report_birth(&[row], true, true, &native),
    );
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
fn born_raw_unknown_lifecycle_booleans_refuse() {
    // Unknown lifecycle facts never fabricate: a non-boolean `managed`
    // refuses as a value, a missing `deleted` refuses as malformed, the
    // queued command stays pending, and the next valid capture still mints
    // the unskipped revision.
    let (service, generation) = discovered_service();
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
    let good = born_maximized_row(BORN_WIN_C, UUID_A);
    let bad_managed = good.replace("\"managed\":true", "\"managed\":\"yes\"");
    let refused = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(&[bad_managed], true, true, &native),
            ))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "unknown-value");
    let bad_missing = good.replace(",\"deleted\":false", "");
    let refused = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                2,
                2,
                &inner_report_birth(&[bad_missing], true, true, &native),
            ))
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

fn preview_windows() -> Vec<String> {
    vec![
        window_json("win-001", "[0,0,960,1040]", None),
        window_json("win-002", "[960,0,960,1040]", None),
    ]
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
/// the producing-gesture identity (nulls when the producing native gesture
/// is unproven, e.g. a command-produced preview with no gesture lane: the
/// geometry still rides the shared projection while the renderer hides it,
/// fail closed).
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

/// A command-produced preview names no producing gesture: both producer
/// fields ride as explicit nulls (unproven, fail closed at the renderer).
fn assert_unproven_producer(preview: &serde_json::Value) {
    assert!(
        preview.get("producerInstance").is_some_and(|v| v.is_null()),
        "unproven preview names no instance"
    );
    assert!(
        preview.get("producerSerial").is_some_and(|v| v.is_null()),
        "unproven preview names no serial"
    );
}
fn drag_preview_command() -> &'static str {
    "{\"op\":\"operation\",\"command\":{\"op\":\"drag-preview\",\
      \"win\":\"win-001\",\"x\":155,\"y\":50,\"hover\":null,\"source\":null}}"
}

#[test]
fn drop_preview_projection_is_shared_read_only_and_cleared_by_fresh_facts() {
    // Stage 5 consumer contract: a drained drag-preview command settles its
    // terminal result requester-only (sender-bound take, hover chaining
    // included), while the shared read-only presentation carries only the
    // trimmed renderer geometry beside the snapshot (never the result lane,
    // never hover, never write tokens). The next preview-less accept clears
    // it: the last accepted capture wins.
    let (service, generation) = discovered_service();
    let inner = inner_report(&preview_windows(), true, false, "");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("admit");
    service
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "drag-1", "1", drag_preview_command()),
        )
        .expect("queued");
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("preview drained"),
    );
    assert_eq!(drained["rev"], "2");
    assert!(
        drained["result"].is_null(),
        "actuator lane keeps no terminal result"
    );
    let ready = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "drag-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready["outcome"], "ready");
    assert_eq!(ready["result"]["op"], "drag-preview");
    assert!(ready["result"]["rect"].is_array());
    assert!(
        ready["result"].get("hover").is_some(),
        "issuer lane keeps hover chaining for the next preview or drop"
    );
    // A foreign sender cannot collect the issuer's terminal result.
    let foreign = parse(
        &service
            .take_result(SENDER_B, &take_req(&generation, "drag-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(foreign["outcome"], "rejected");
    assert_eq!(foreign["kind"], "unknown-command");
    // Any same-UID subscriber reads the shared trimmed preview beside the
    // snapshot: paint geometry plus subject proof, no result lane, no hover
    // chain, no write tokens. Same geometry the issuer collected through
    // take, minus issuer correlation.
    let projection = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"sub-1\"}")
            .expect("in-band"),
    );
    assert_eq!(projection["outcome"], "presentation");
    assert_eq!(
        projection["gen"].as_str(),
        Some(generation.as_str()),
        "read binds the serving generation"
    );
    assert!(projection.get("result").is_none());
    assert_trimmed_preview(&projection["preview"]);
    assert_unproven_producer(&projection["preview"]);
    assert_eq!(projection["preview"]["rect"], ready["result"]["rect"]);
    // Subject proof: the neighbor identity names a live row of the same
    // cached snapshot, and the mover-home domain matches the issuer lane.
    let live: Vec<&str> = projection["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .filter_map(|row| row.get("win").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        projection["preview"]["win"]
            .as_str()
            .is_some_and(|win| live.contains(&win)),
        "preview subject names a live snapshot row"
    );
    assert_eq!(
        projection["preview"]["sourceWs"],
        ready["result"]["sourceWs"]
    );
    for window in projection["windows"].as_array().expect("windows") {
        assert!(window.get("frame_op").is_none());
        assert!(window.get("clear_op").is_none());
    }
    // A preview-less follow-up accept clears the shared preview (drag
    // termination): last accepted capture wins, snapshot stays fresh.
    let next = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
            .expect("accepted"),
    );
    assert_eq!(next["rev"], "3");
    let cleared = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"sub-2\"}")
            .expect("in-band"),
    );
    assert_eq!(cleared["outcome"], "presentation");
    assert!(cleared["preview"].is_null());
    assert_eq!(cleared["rev"], "3");
    // A non-preview terminal never rides the preview lane.
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
    let toggled = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 4, 4, &inner))
            .expect("drained"),
    );
    assert!(toggled["result"].is_null());
    let settled = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(settled["outcome"], "ready");
    assert_eq!(settled["result"]["op"], "toggle-float");
    let projection = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"sub-3\"}")
            .expect("in-band"),
    );
    assert!(
        projection["preview"].is_null(),
        "only drop previews ride the preview lane"
    );
}

#[test]
fn preview_survives_rejected_captures_but_fallback_clears_it() {
    // Freshness: in-band rejections never touch the cached preview, while a
    // failed-drain fallback accept publishes its own preview-less
    // projection (last accepted capture wins, never a stale preview).
    let (service, generation) = discovered_service();
    let inner = inner_report(&preview_windows(), true, false, "");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("admit");
    service
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "drag-1", "1", drag_preview_command()),
        )
        .expect("queued");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
        .expect("preview drained");
    let seeded = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"seed\"}")
            .expect("in-band"),
    );
    assert_trimmed_preview(&seeded["preview"]);
    assert_unproven_producer(&seeded["preview"]);
    // A malformed envelope rejects without touching the cache.
    let malformed = parse(&service.evaluate_capture("{\"v\":2}").expect("in-band"));
    assert_eq!(malformed["outcome"], "rejected");
    // A remembered source with different bytes refuses as reused.
    let reused = parse(
        &service
            .evaluate_capture(&capture_source(
                ADAPTER,
                &generation,
                &js_source(2),
                2,
                2,
                &inner_report(&preview_windows(), false, false, ""),
            ))
            .expect("in-band"),
    );
    assert_eq!(reused["outcome"], "rejected");
    assert_eq!(reused["kind"], "source-reused");
    let kept = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"kept\"}")
            .expect("in-band"),
    );
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
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
            .expect("fallback accepted"),
    );
    assert_eq!(fallback["rev"], "3");
    assert!(fallback["result"].is_null());
    let failed = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(failed["outcome"], "failed");
    let cleared = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"after-fallback\"}")
            .expect("in-band"),
    );
    assert_eq!(cleared["outcome"], "presentation");
    assert_eq!(cleared["rev"], "3");
    assert!(
        cleared["preview"].is_null(),
        "fallback accept publishes no stale preview"
    );
}

const UUID_C: &str = "6ba7b811-9dad-11d1-80b4-00c04fd430c8";
const UUID_D: &str = "6ba7b812-9dad-11d1-80b4-00c04fd430c8";

/// Two-output producer-shaped inner report for the all-mode Stage 7 probe:
/// DP-1 plus DP-2, host-owned, equal count on, the same global 4-native
/// order on both outputs with a shared current, and the caller-retained
/// window riding DP-2's fourth native (B/ws4 occupied, B1-3 empty).
fn two_output_all_mode_inner(mode: &str, window: &str) -> String {
    format!(
        "{{\"v\":2,\"mode\":\"{mode}\",\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"gap\":8,\"outer\":4,\"complete\":true,\"primary\":true}},\
          {{\"out\":\"DP-2\",\"full\":[1920,0,1920,1080],\
          \"work\":[1920,0,1920,1040],\"gap\":8,\"outer\":4,\"complete\":true,\"primary\":false}}],\
          \"windows\":[{window}],\
          \"views\":[{{\"out\":\"DP-1\",\"view\":\"\"}},{{\"out\":\"DP-2\",\"view\":\"\"}}],\"closed\":[],\
          \"hostOwned\":true,\"equalCount\":true,\
          \"native\":[{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\",\"{UUID_C}\",\"{UUID_D}\"],\
          \"current\":\"{UUID_A}\"}},\
          {{\"out\":\"DP-2\",\"order\":[\"{UUID_A}\",\"{UUID_B}\",\"{UUID_C}\",\"{UUID_D}\"],\
          \"current\":\"{UUID_A}\"}}],\
          \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
          \"defaultTiled\":true,\"focused\":\"\",\
          \"complete\":true,\"gesture\":false,\"switchTarget\":\"cursor-output\",\
          \"freshPlacement\":\"cursor-output-last-focused\",\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\",\"DP-2\"],\"focusFollowsCursor\":false,\"cursorWarp\":false,\
          \"presentation\":{}}}",
        disabled_presentation()
    )
}

#[test]
fn all_modes_two_output_native_rings_through_service() {
    // Stage 7 all-mode REAL broker/native outcomes through WorldService: the
    // same producer-shaped raw report (2 outputs, one global 4-native list,
    // equalCount on, retained win-b on DP-2's fourth native) under each
    // workspace mode. Separate modes keep disjoint per-output logical rings
    // with distinct current receipts; shared links one ring and one view.
    // Equal host counts pad to equal length but never link: equal count is
    // not linked mode. Local vs global-unique share one public shape here
    // (the swap discriminator lives at engine-command level, not in this
    // reply); shared is the observed distinction.
    let window = window_json("win-b", "[1920,0,960,1040]", Some(UUID_D))
        .replace("\"out\":\"DP-1\"", "\"out\":\"DP-2\"");
    for mode in ["per-output-local", "global-unique", "shared"] {
        let (service, generation) = discovered_service();
        let reply = parse(
            &service
                .evaluate_capture(&capture(
                    ADAPTER,
                    &generation,
                    1,
                    1,
                    &two_output_all_mode_inner(mode, &window),
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
        // Equal host counts pad to equal length on every mode.
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
    // and the new lifetime re-seeds its own latest preview after
    // re-discovery. The old preview never leaks across.
    let service = WorldService::new();
    let inner = inner_report(&preview_windows(), true, false, "");
    let boot_a = parse(&service.discovery_body("lifetime-a", "req-0"));
    let gen_a = boot_a["gen"].as_str().expect("gen").to_owned();
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
    service
        .queue_command(
            SENDER_A,
            &queue_req(&gen_a, "drag-1", "1", drag_preview_command()),
        )
        .expect("queued");
    service
        .evaluate_capture(&capture_source(
            "lifetime-a",
            &gen_a,
            &js_source(2),
            2,
            2,
            &inner,
        ))
        .expect("preview drained");
    let seeded = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"seed-a\"}")
            .expect("in-band"),
    );
    assert_trimmed_preview(&seeded["preview"]);
    assert_unproven_producer(&seeded["preview"]);
    assert_eq!(seeded["gen"].as_str(), Some(gen_a.as_str()));
    let boot_b = parse(&service.discovery_body("lifetime-b", "req-1"));
    let gen_b = boot_b["gen"].as_str().expect("gen").to_owned();
    assert_ne!(gen_b, gen_a);
    let not_ready = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"seed-b\"}")
            .expect("in-band"),
    );
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
    // The new lifetime re-seeds from its own first preview-carrying accept.
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
    service
        .queue_command(
            SENDER_A,
            &queue_req(&gen_b, "drag-1", "1", drag_preview_command()),
        )
        .expect("queued");
    service
        .evaluate_capture(&capture_source(
            "lifetime-b",
            &gen_b,
            &js_source(2),
            2,
            2,
            &inner,
        ))
        .expect("preview drained");
    let reseeded = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"seed-b2\"}")
            .expect("in-band"),
    );
    assert_eq!(reseeded["outcome"], "presentation");
    assert_trimmed_preview(&reseeded["preview"]);
    assert_unproven_producer(&reseeded["preview"]);
    assert_eq!(reseeded["gen"].as_str(), Some(gen_b.as_str()));
}
