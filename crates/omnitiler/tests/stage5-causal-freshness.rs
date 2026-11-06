//! Stage 5 broker-projection causal-freshness contracts (component of the
//! full proof).
//!
//! These drive the production `WorldService` capture/presentation route
//! (discovery, `evaluate_capture`, `read_presentation`) through executed
//! capture sequences, including gesture A-terminal/core settled/B start and
//! a queued drag-preview command drain. They pin the broker contract: the
//! cached presentation names the covered native-invalidation witness
//! (effect-owned counter, never the JS transport watermark) and every
//! served drop preview names its producing native gesture identity, or
//! nulls when that identity is unproven.
//!
//! The envelope `nativeInvalid` watermark is deliberately diverged from
//! the witness counters below: the broker must never alias the JS-owned
//! transport watermark to the effect-owned causal witness.
//!
//! The render-hiding half of each interleaving (stale paint must stay
//! hidden, cross-gesture preview must not paint, in-transit paint must not
//! resurrect) is proven at the production entry route in
//! `kwin/tests/world-entry-freshness.test.ts`, which drives
//! `startWorldAdapterEntry` signals/captures/reads/renders; the native
//! consumer half lives in
//! `kwin/native-effect/presentationconsumer_test.cpp`.

use omnitiler::world_service::WorldService;

const ADAPTER: &str = "kwin-actuator-1";
const SENDER: &str = ":1.42";

fn v4_win(id: &str, rect: &str) -> String {
    let frame = rect
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\"rect\":[{x},{y},{w},{h}],\
          \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\
          \"minimized\":false,\
          \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
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

fn window_json(id: &str, rect: &str) -> String {
    v4_win(id, rect)
}

fn bare_window_json(id: &str, rect: &str) -> String {
    v4_win(id, rect)
}

fn inner_report(windows: &[String]) -> String {
    format!(
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[],\"native\":[],\
          \"focused\":\"\",\"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
          \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\
          \"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\
          \"pagerOrder\":[\"win-a\",\"win-b\"],\
          \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}},\
          {{\"win\":\"win-b\",\"live\":[300,0,200,200]}}],\
          \"witness\":null,\"foregroundOut\":null}},\
          \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
          \"rendererInput\":null,\
          \"gestureIngress\":null,\"nativeGesture\":null,\
          \"cursorWindow\":null,\"nativeInvalidWitness\":null}}",
        windows.join(","),
    )
}

/// Attach the raw native-invalidation witness lane (effect-owned causal
/// counter) to one inner report. The broker validates and strips it before
/// the strict core decode; the served projection carries it back as the
/// covered value.
fn witnessed(inner: &str, instance: &str, counter: &str) -> String {
    inner.replacen(
        "\"nativeInvalidWitness\":null",
        &format!(
            "\"nativeInvalidWitness\":{{\"instance\":\"{instance}\",\"counter\":\"{counter}\"}}"
        ),
        1,
    )
}

fn capture(
    service: &WorldService,
    generation: &str,
    seq: u64,
    watermark: u64,
    inner: &str,
) -> serde_json::Value {
    let envelope = format!(
        "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"source\":\"src-{seq}-t\",\"sourceSeq\":\"{seq}\",\
          \"nativeInvalid\":\"{watermark}\",\"report\":{inner}}}"
    );
    let reply = service.evaluate_capture(&envelope).expect("in-band");
    serde_json::from_str(&reply).expect("reply is JSON")
}

fn read_presentation(service: &WorldService, req: &str) -> serde_json::Value {
    let reply = service
        .read_presentation(&format!("{{\"v\":4,\"req\":\"{req}\"}}"))
        .expect("in-band");
    serde_json::from_str(&reply).expect("reply is JSON")
}

/// v4 connection handshake for the discovery route (dispatched by type
/// through the connection router). Producer lifetime is the opaque
/// actuator incarnation; every handshake resets onto a fresh generation.
fn handshake_req(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"birthClear\":false,\
          \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
          \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
          \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
    )
}

fn apply_settings(service: &WorldService, generation: &str) {
    let body = format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"req\":\"settings-1\",\"rev\":\"1\",\"values\":{{}}}}"
    );
    let reply: serde_json::Value =
        serde_json::from_str(&service.discover(&body).expect("settings routes"))
            .expect("valid JSON");
    assert_eq!(reply["status"], "accepted");
}

fn handshake_discover(service: &WorldService, adapter: &str, req: &str) -> String {
    service
        .discover(&handshake_req(adapter, req))
        .expect("handshake routes")
}

fn discovered() -> (WorldService, String) {
    let service = WorldService::new();
    let discovery: serde_json::Value =
        serde_json::from_str(&handshake_discover(&service, ADAPTER, "req-0")).expect("valid JSON");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    apply_settings(&service, &generation);
    (service, generation)
}

fn gesture_open_inner(windows: &[String], instance: &str, serial: &str) -> String {
    inner_report(windows)
        .replacen(
            "\"gestureIngress\":null,\"nativeGesture\":null",
            &format!(
                "\"gestureIngress\":null,\"nativeGesture\":{{\"v\":1,\"instance\":\"{instance}\",\"serial\":\"{serial}\",\"revision\":\"1\",\"phase\":\"updated\",\"win\":\"win-a\",\"kind\":\"move\",\"startRect\":[0,0,200,200],\"startCursor\":[10,10],\"cursor\":null,\"modalMove\":1,\"changed\":1,\"superseded\":0,\"available\":1}}"
            ),
            1,
        )
        .replacen("\"gesture\":false", "\"gesture\":true", 1)
        .replacen("\"cursor\":null", "\"cursor\":[150,50]", 1)
}

fn gesture_terminal_inner(windows: &[String], instance: &str, serial: &str) -> String {
    inner_report(windows)
        .replacen(
            "\"gestureIngress\":null,\"nativeGesture\":null",
            &format!(
                "\"gestureIngress\":null,\"nativeGesture\":{{\"v\":1,\"instance\":\"{instance}\",\"serial\":\"{serial}\",\"revision\":\"2\",\"phase\":\"released\",\"win\":\"win-a\",\"kind\":\"move\",\"startRect\":[0,0,200,200],\"startCursor\":[10,10],\"cursor\":[30,30],\"modalMove\":1,\"changed\":1,\"superseded\":0,\"available\":1}}"
            ),
            1,
        )
        .replacen("\"gesture\":false", "\"gesture\":true", 1)
        .replacen("\"cursor\":null", "\"cursor\":[30,30]", 1)
}

/// Row 1: geometry dirt versus in-flight newer snapshot.
///
/// Executed sequence: accept C1 (witness effect-1/1, unrelated transport
/// watermark 10), accept C2 (same witness, pre-move frame), read R_a; then
/// accept C3 (witness effect-1/2, post-move frame), read R_b. Both served
/// projections must name the captured witness (not the transport
/// watermark) so a renderer hides R_a once the latest witness is 2.
#[test]
fn stage5_red_geometry_dirt_requires_covered_watermark() {
    let (service, generation) = discovered();
    let c1 = capture(
        &service,
        &generation,
        1,
        10,
        &witnessed(
            &inner_report(&[window_json("win-a", "[10,10,300,300]")]),
            "effect-1",
            "1",
        ),
    );
    assert_eq!(c1["nativeInvalid"], serde_json::json!("10"));
    let c2 = capture(
        &service,
        &generation,
        2,
        11,
        &witnessed(
            &inner_report(&[window_json("win-a", "[10,10,300,300]")]),
            "effect-1",
            "1",
        ),
    );
    assert_eq!(c2["nativeInvalid"], serde_json::json!("11"));
    let ra = read_presentation(&service, "red-geometry-a");
    assert_eq!(ra["outcome"], serde_json::json!("presentation"));
    let c3 = capture(
        &service,
        &generation,
        3,
        12,
        &witnessed(
            &inner_report(&[window_json("win-a", "[60,10,300,300]")]),
            "effect-1",
            "2",
        ),
    );
    assert_eq!(c3["nativeInvalid"], serde_json::json!("12"));
    let rb = read_presentation(&service, "red-geometry-b");
    assert_eq!(rb["outcome"], serde_json::json!("presentation"));
    let mut missing = Vec::new();
    if ra.get("coveredInstance") != Some(&serde_json::json!("effect-1")) {
        missing.push("pre-move read must name the covering witness instance");
    }
    if ra.get("coveredInvalid") != Some(&serde_json::json!("1")) {
        missing.push("pre-move read must publish coveredInvalid==1 (live 2 stays hidden)");
    }
    if rb.get("coveredInstance") != Some(&serde_json::json!("effect-1")) {
        missing.push("post-move read must name the covering witness instance");
    }
    if rb.get("coveredInvalid") != Some(&serde_json::json!("2")) {
        missing.push("post-move read must publish coveredInvalid==2");
    }
    assert!(
        missing.is_empty(),
        "broker projection lacks coverage proof: {}",
        missing.join("; ")
    );
}

/// Row 2: preview publication versus gesture boundary.
///
/// Executed sequence: open gesture A (effect identity gesture-1/serial 1)
/// with a live script cursor; the bound capture reply carries the
/// broker-mapped association and the cached projection carries a drop
/// preview from THIS dispatch. Then finish A (released terminal, settled
/// against the same association) and start B (serial 2): the cached B
/// preview must name B, never A. Finally a queued drag-preview command
/// drained with no gesture lane must ride its geometry with a null
/// producer (unproven, fail closed at the renderer) while the issuer lane
/// still settles.
#[test]
fn stage5_red_preview_requires_producing_identity() {
    let (service, generation) = discovered();
    let wins = [
        bare_window_json("win-a", "[0,0,200,200]"),
        bare_window_json("win-b", "[300,0,200,200]"),
    ];
    let opened = capture(
        &service,
        &generation,
        1,
        10,
        &witnessed(
            &gesture_open_inner(&wins, "gesture-1", "1"),
            "effect-1",
            "1",
        ),
    );
    assert_eq!(opened["nativeInvalid"], serde_json::json!("10"));
    let assoc = opened["gestureAssoc"].clone();
    assert_eq!(assoc["instance"], serde_json::json!("gesture-1"));
    assert_eq!(assoc["serial"], serde_json::json!("1"));
    let cached = read_presentation(&service, "red-preview-a");
    assert_eq!(cached["outcome"], serde_json::json!("presentation"));
    let preview_a = cached["preview"].clone();
    assert!(
        !preview_a.is_null(),
        "expected a cached drop preview for the open gesture, got {cached}"
    );
    let terminal = capture(
        &service,
        &generation,
        2,
        11,
        &witnessed(
            &gesture_terminal_inner(&wins, "gesture-1", "1"),
            "effect-1",
            "1",
        ),
    );
    let settled = terminal["gestureSettled"].clone();
    assert_eq!(settled["id"], assoc["coreId"]);
    let started_b = capture(
        &service,
        &generation,
        3,
        12,
        &witnessed(
            &gesture_open_inner(&wins, "gesture-1", "2"),
            "effect-1",
            "1",
        ),
    );
    assert_eq!(started_b["gestureAssoc"]["serial"], serde_json::json!("2"));
    let cached_b = read_presentation(&service, "red-preview-b");
    assert_eq!(cached_b["outcome"], serde_json::json!("presentation"));
    let preview_b = cached_b["preview"].clone();
    assert!(
        !preview_b.is_null(),
        "expected a cached drop preview for the live gesture B, got {cached_b}"
    );
    let mut missing = Vec::new();
    if preview_a.get("producerInstance") != Some(&serde_json::json!("gesture-1")) {
        missing.push("cached A preview must name producing instance gesture-1");
    }
    if preview_a.get("producerSerial") != Some(&serde_json::json!("1")) {
        missing.push("cached A preview must name producing serial 1");
    }
    if preview_b.get("producerInstance") != Some(&serde_json::json!("gesture-1")) {
        missing.push("cached B preview must name producing instance gesture-1, not the neighbor");
    }
    if preview_b.get("producerSerial") != Some(&serde_json::json!("2")) {
        missing.push("cached B preview must name producing serial 2, never the finished A");
    }
    assert!(
        missing.is_empty(),
        "broker projection lacks producing-gesture proof: {}",
        missing.join("; ")
    );

    // Queued unrelated drag-preview command drained with no gesture lane:
    // the geometry still rides the shared projection, but the producer is
    // null (unproven, fail closed at the renderer) while the issuer lane
    // settles the terminal result.
    let (commanded, command_gen) = discovered();
    let big = [
        bare_window_json("win-a", "[0,0,960,1040]"),
        bare_window_json("win-b", "[960,0,960,1040]"),
    ];
    commanded
        .evaluate_capture(&format!(
            "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{command_gen}\",\
              \"source\":\"src-1-t\",\"sourceSeq\":\"1\",\
              \"nativeInvalid\":\"10\",\"report\":{}}}",
            witnessed(&inner_report(&big), "effect-1", "1")
        ))
        .expect("admit");
    // v4 inherent boundary: the queue holds no drag-preview vocabulary
    // (the `operation` wrapper refuses), so the command-only leg fails
    // closed at queue time and never produces a preview. Live gestures
    // stay the only preview source; the issuer lane settles nothing.
    let drag_command = "{\"op\":\"operation\",\"command\":{\"op\":\"drag-preview\",\
        \"win\":\"win-a\",\"x\":155,\"y\":50,\"hover\":null,\"source\":null}}";
    let queue_reply: serde_json::Value = serde_json::from_str(
        &commanded
            .queue_command(
                SENDER,
                &format!(
                    "{{\"v\":4,\"gen\":\"{command_gen}\",\"client\":\"drag-1\",\
                      \"clientCmd\":\"1\",\"command\":{drag_command}}}"
                ),
            )
            .expect("in-band"),
    )
    .expect("queue reply is JSON");
    assert_eq!(queue_reply["outcome"], serde_json::json!("rejected"));
    assert_eq!(queue_reply["kind"], serde_json::json!("snapshot-invalid"));
    let drained: serde_json::Value = serde_json::from_str(
        &commanded
            .evaluate_capture(&format!(
                "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{command_gen}\",\
                  \"source\":\"src-2-t\",\"sourceSeq\":\"2\",\
                  \"nativeInvalid\":\"11\",\"report\":{}}}",
                witnessed(&inner_report(&big), "effect-1", "1")
            ))
            .expect("preview drained"),
    )
    .expect("drained is JSON");
    assert!(
        drained["result"].is_null(),
        "actuator lane keeps no terminal result"
    );
    assert!(
        drained["gestureAssoc"].is_null(),
        "no gesture lane means no bound association"
    );
    let settled: serde_json::Value = serde_json::from_str(
        &commanded
            .take_result(
                SENDER,
                &format!(
                    "{{\"v\":4,\"gen\":\"{command_gen}\",\"client\":\"drag-1\",\"clientCmd\":\"1\"}}"
                ),
            )
            .expect("in-band"),
    )
    .expect("settled is JSON");
    assert_eq!(settled["outcome"], serde_json::json!("rejected"));
    assert_eq!(settled["kind"], serde_json::json!("unknown-command"));
    let projection = read_presentation(&commanded, "red-preview-command");
    assert_eq!(projection["outcome"], serde_json::json!("presentation"));
    let preview_c = projection["preview"].clone();
    assert!(
        preview_c.is_null(),
        "v4 holds no command-produced preview vocabulary, got {projection}"
    );
}

/// Row 3: focus clear versus in-transit paint, plus the error-hide path.
///
/// Executed sequence: show A (accept, witness effect-1/1), read R1; drive
/// the in-band error path (same source, different bytes: `source-reused`
/// rejection, which touches neither the presentation cache nor the
/// coverage), read R2; advance the witness as an activation would (accept,
/// witness effect-1/2), read R3. Every served projection must name its
/// covered witness so an in-transit paint delivered after the clear
/// cannot resurrect: the renderer keeps the clear until a covering capture
/// arrives. All three reads lack the key today.
#[test]
fn stage5_red_focus_clear_preserves_covered_watermark() {
    let (service, generation) = discovered();
    let shown = capture(
        &service,
        &generation,
        1,
        10,
        &witnessed(
            &inner_report(&[window_json("win-a", "[10,10,300,300]")]),
            "effect-1",
            "1",
        ),
    );
    assert_eq!(shown["nativeInvalid"], serde_json::json!("10"));
    let r1 = read_presentation(&service, "red-focus-1");
    assert_eq!(r1["outcome"], serde_json::json!("presentation"));
    let inner = witnessed(
        &inner_report(&[window_json("win-a", "[10,10,300,300]")]),
        "effect-1",
        "1",
    );
    let envelope = format!(
        "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"source\":\"src-1-t\",\"sourceSeq\":\"1\",\
          \"nativeInvalid\":\"10\",\"report\":{inner}}}"
    );
    let hostile = envelope.replace("[10,10,300,300]", "[11,10,300,300]");
    let rejected: serde_json::Value = serde_json::from_str(
        &service
            .evaluate_capture(&hostile)
            .expect("in-band rejection"),
    )
    .expect("rejection is JSON");
    assert_eq!(rejected["outcome"], serde_json::json!("rejected"));
    assert_eq!(rejected["kind"], serde_json::json!("source-reused"));
    let r2 = read_presentation(&service, "red-focus-2");
    assert_eq!(r2["outcome"], serde_json::json!("presentation"));
    let activated = capture(
        &service,
        &generation,
        2,
        11,
        &witnessed(
            &inner_report(&[window_json("win-a", "[10,10,300,300]")]),
            "effect-1",
            "2",
        ),
    );
    assert_eq!(activated["nativeInvalid"], serde_json::json!("11"));
    let r3 = read_presentation(&service, "red-focus-3");
    assert_eq!(r3["outcome"], serde_json::json!("presentation"));
    let mut missing = Vec::new();
    if r1.get("coveredInvalid") != Some(&serde_json::json!("1")) {
        missing.push("A paint must preserve coveredInvalid==1 across the error path");
    }
    if r2.get("coveredInvalid") != Some(&serde_json::json!("1")) {
        missing.push("post-error read must still cover 1 (no silent reset)");
    }
    if r3.get("coveredInvalid") != Some(&serde_json::json!("2")) {
        missing.push("post-activation read must cover 2");
    }
    for (name, read) in [("r1", &r1), ("r2", &r2), ("r3", &r3)] {
        if read.get("coveredInstance") != Some(&serde_json::json!("effect-1")) {
            missing.push(match name {
                "r1" => "r1 must name the covering witness instance",
                "r2" => "r2 must name the covering witness instance",
                _ => "r3 must name the covering witness instance",
            });
        }
    }
    assert!(
        missing.is_empty(),
        "broker projection lacks coverage proof: {}",
        missing.join("; ")
    );
}
