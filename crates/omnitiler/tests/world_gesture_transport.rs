//! Broker-path gesture lifecycle transport: the production WorldService
//! capture lane folds the effect-owned raw native lane (snapshot plus
//! same-capture script cursor/windowAt facts) into core ingress in the
//! broker, the first terminal with frozen start facts settles exactly
//! once, replays stay dead, and a held open clears a stale drop preview.

use omnitiler::world_service::WorldService;

const ADAPTER: &str = "kwin-actuator-1";

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

fn inner_report(windows: &[String]) -> String {
    format!(
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[],\"native\":[],\
          \"focused\":\"\",\"foregroundKnown\":false,\"complete\":true,\"gesture\":false,\
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

fn with_ingress(inner: &str, ingress: &str) -> String {
    inner.replacen(
        "\"gestureIngress\":null",
        &format!("\"gestureIngress\":{ingress}"),
        1,
    )
}

fn ingress(id: &str, phase: &str, cursor: &str, start_cursor: &str, start: &str) -> String {
    format!(
        "{{\"win\":\"win-a\",\"kind\":\"move\",\"phase\":\"{phase}\",\"id\":\"{id}\",\
          \"cursor\":{cursor},\"startCursor\":{start_cursor},\"esc\":false,\"modal\":true,\
          \"start\":{start}}}"
    )
}

fn capture(service: &WorldService, generation: &str, seq: u64, inner: &str) -> serde_json::Value {
    let envelope = capture_envelope_text(generation, seq, inner);
    let reply = service.evaluate_capture(&envelope).expect("in-band");
    serde_json::from_str(&reply).expect("reply is JSON")
}

fn capture_envelope_text(generation: &str, seq: u64, inner: &str) -> String {
    format!(
        "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"source\":\"src-{seq}-t\",\"sourceSeq\":\"{seq}\",\
          \"nativeInvalid\":\"{seq}\",\"report\":{inner}}}"
    )
}

// ---- Broker-owned gesture normalization (raw native lane) ----

#[allow(clippy::too_many_arguments)]
fn raw_native_full(
    instance: &str,
    serial: &str,
    revision: &str,
    phase: &str,
    win: &str,
    kind: &str,
    start_rect: &str,
    start_cursor: &str,
    cursor: &str,
    modal: u8,
    changed: u8,
    superseded: u8,
    available: u8,
) -> String {
    format!(
        "{{\"v\":1,\"instance\":\"{instance}\",\"serial\":\"{serial}\",\"revision\":\"{revision}\",\
          \"phase\":\"{phase}\",\"win\":\"{win}\",\"kind\":\"{kind}\",\
          \"startRect\":{start_rect},\"startCursor\":{start_cursor},\"cursor\":{cursor},\
          \"modalMove\":{modal},\"changed\":{changed},\"superseded\":{superseded},\"available\":{available}}}"
    )
}

fn raw_open(serial: &str, revision: &str, cursor: &str) -> String {
    raw_native_full(
        "gesture-1",
        serial,
        revision,
        "updated",
        "win-a",
        "move",
        "[0,0,200,200]",
        "[10,10]",
        cursor,
        1,
        1,
        0,
        1,
    )
}

fn raw_terminal(serial: &str, revision: &str, cursor: &str) -> String {
    raw_native_full(
        "gesture-1",
        serial,
        revision,
        "released",
        "win-a",
        "move",
        "[0,0,200,200]",
        "[10,10]",
        cursor,
        1,
        1,
        0,
        1,
    )
}

fn with_raw(inner: &str, native: &str, cursor: &str, cursor_window: &str) -> String {
    let stepped = inner.replacen(
        "\"gestureIngress\":null,\"nativeGesture\":null",
        &format!("\"gestureIngress\":null,\"nativeGesture\":{native}"),
        1,
    );
    let stepped = stepped.replacen(
        "\"cursorWindow\":null",
        &format!("\"cursorWindow\":{cursor_window}"),
        1,
    );
    let stepped = stepped.replacen("\"gesture\":false", "\"gesture\":true", 1);
    stepped.replacen("\"cursor\":null", &format!("\"cursor\":{cursor}"), 1)
}

/// v4 connection handshake for the discovery route (dispatched by type
/// through the connection router). Producer lifetime is the opaque
/// actuator incarnation; every handshake resets onto a fresh generation.
fn handshake_req(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
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

fn discover(service: &WorldService) -> String {
    let discovery: serde_json::Value =
        serde_json::from_str(&handshake_discover(service, ADAPTER, "req-0")).expect("valid JSON");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    apply_settings(service, &generation);
    generation
}

fn preview_of(service: &WorldService, req: &str) -> serde_json::Value {
    serde_json::from_str(
        &service
            .read_presentation(&format!("{{\"v\":4,\"req\":\"{req}\"}}"))
            .expect("in-band"),
    )
    .expect("valid JSON")
}

#[test]
fn open_folds_live_script_cursor_and_terminal_freezes_native() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();

    // Open with a null native cursor folds the live script cursor: moved
    // script pointer previews.
    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "1",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "null",
            1,
            1,
            0,
            1,
        ),
        "[150,50]",
        "null",
    );
    let replied = capture(&service, &generation, 1, &open);
    assert_eq!(replied["outcome"], serde_json::Value::Null);
    let projection = preview_of(&service, "read-fold-1");
    assert!(
        !projection["preview"].is_null(),
        "open folds live script cursor, got {projection}"
    );

    // Terminal with a null native cursor ignores a later script pointer:
    // the frozen finish cursor stays null, so a moved frame without a
    // cursor cannot commit a move drop (committed false proves the script
    // pointer was never folded in; folding it would commit).
    let moved_a = "[100,0,200,200]".to_owned();
    let terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "2", "null"),
        "[999,999]",
        "null",
    );
    let settled = capture(&service, &generation, 2, &terminal);
    assert_eq!(
        settled["gestureSettled"],
        serde_json::json!({"id": "1", "committed": false}),
        "terminal freezes the native finish cursor, got {settled}"
    );
}

#[test]
fn stale_serial_never_gets_a_newer_core_id() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    // Newer serial first: open then terminal settle as core id 1.
    let open2 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("2", "1", "[150,50]"),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 1, &open2);
    let terminal2 = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("2", "2", "[150,50]"),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 2, &terminal2);
    assert_eq!(settled["gestureSettled"]["id"], serde_json::json!("1"));

    // Older unseen serial arriving later never mints a newer core id: the
    // terminal omits ingress and retains the prior settled outcome unchanged.
    let stale = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "3", "[150,50]"),
        "null",
        "null",
    );
    let omitted = capture(&service, &generation, 3, &stale);
    assert_eq!(omitted["outcome"], serde_json::Value::Null);
    assert!(
        omitted["gestureAssoc"].is_null(),
        "stale serial omits ingress, got {omitted}"
    );
    assert_eq!(
        omitted["gestureSettled"], settled["gestureSettled"],
        "stale serial retains prior settlement unchanged, got {omitted}"
    );
    assert_eq!(
        omitted["gestureSettledAssoc"], settled["gestureSettledAssoc"],
        "stale serial retains prior settled association unchanged, got {omitted}"
    );
}

#[test]
fn start_contradiction_refuses_without_map_mutation() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    let replied = capture(&service, &generation, 1, &open);
    assert_eq!(replied["outcome"], serde_json::Value::Null);

    // Same native identity with different frozen start facts fails closed.
    let contradicted = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "2",
            "released",
            "win-a",
            "move",
            "[5,5,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let refused = capture(&service, &generation, 2, &contradicted);
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");

    // The map is unmutated: the genuine terminal still settles as id 1.
    let terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "3", "[150,50]"),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 3, &terminal);
    assert_eq!(
        settled["gestureSettled"],
        serde_json::json!({"id": "1", "committed": true}),
        "contradiction leaves the map intact, got {settled}"
    );
}

#[test]
fn retired_source_instance_never_rebinds_late_replies() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    // Instance A opens and settles first (broker core id 1).
    let a_open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-a",
            "1",
            "1",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 1, &a_open);
    let a_terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-a",
            "1",
            "2",
            "released",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let settled_a = capture(&service, &generation, 2, &a_terminal);
    assert_eq!(settled_a["gestureSettled"]["id"], serde_json::json!("1"));

    // Instance B restarts the source and settles distinctly (core id 2):
    // A retires.
    let b_open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-b",
            "1",
            "3",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[160,60]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 3, &b_open);
    let b_terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-b",
            "1",
            "4",
            "released",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[160,60]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let settled_b = capture(&service, &generation, 4, &b_terminal);
    assert_eq!(settled_b["gestureSettled"]["id"], serde_json::json!("2"));

    // Late unseen serials from retired A omit in both phases: the open
    // carries no ingress and the terminal settles nothing (never id 3).
    let a_late_open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-a",
            "2",
            "5",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[170,70]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let omitted_open = capture(&service, &generation, 5, &a_late_open);
    assert_eq!(omitted_open["outcome"], serde_json::Value::Null);
    let a_late_terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-a",
            "2",
            "6",
            "released",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[170,70]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let omitted_terminal = capture(&service, &generation, 6, &a_late_terminal);
    assert!(
        omitted_terminal["gestureAssoc"].is_null(),
        "retired terminal omits ingress, got {omitted_terminal}"
    );
    assert_eq!(
        omitted_terminal["gestureSettled"], settled_b["gestureSettled"],
        "retired terminal retains live instance b settlement unchanged, got {omitted_terminal}"
    );
    assert_eq!(
        omitted_terminal["gestureSettledAssoc"], settled_b["gestureSettledAssoc"],
        "retired terminal retains live instance b settled association unchanged, got {omitted_terminal}"
    );

    // The map keeps working after retirement handling: a fresh serial on
    // the live instance allocates the next core id and settles.
    let b3_open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-b",
            "3",
            "7",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[175,75]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let opened_b3 = capture(&service, &generation, 7, &b3_open);
    assert_eq!(opened_b3["outcome"], serde_json::Value::Null);
    let b3_terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-b",
            "3",
            "8",
            "released",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[175,75]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let settled_b3 = capture(&service, &generation, 8, &b3_terminal);
    assert_eq!(
        settled_b3["gestureSettled"]["id"],
        serde_json::json!("3"),
        "map stays live after retirement, got {settled_b3}"
    );
}

#[test]
fn script_minted_ingress_refuses() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();

    // The script never mints: a present gestureIngress beside a raw lane
    // refuses, and a minted lane without any raw lane refuses too.
    let minted_beside_raw = with_ingress(
        &with_raw(
            &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
            &raw_open("1", "1", "[150,50]"),
            "null",
            "null",
        ),
        &ingress("1", "updated", "[150,50]", "[10,10]", &ra),
    );
    let refused = capture(&service, &generation, 1, &minted_beside_raw);
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "unknown-value");

    let minted_alone = with_ingress(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &ingress("1", "updated", "[150,50]", "[10,10]", &ra),
    );
    let refused_alone = capture(&service, &generation, 2, &minted_alone);
    assert_eq!(refused_alone["outcome"], "rejected");
    assert_eq!(refused_alone["kind"], "unknown-value");
}

fn parse(reply: &serde_json::Value) -> &serde_json::Value {
    reply
}

#[test]
fn broker_carries_native_lifecycle_without_second_lifecycle() {
    let service = WorldService::new();
    let discovery: serde_json::Value =
        serde_json::from_str(&handshake_discover(&service, ADAPTER, "req-0")).expect("valid JSON");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    apply_settings(&service, &generation);

    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    // Open update with a moved cursor previews: projection carries a preview.
    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "1",
            "updated",
            "win-a",
            "move",
            &ra,
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let replied = capture(&service, &generation, 1, &open);
    assert_eq!(parse(&replied)["outcome"], serde_json::Value::Null);
    let projection: serde_json::Value = serde_json::from_str(
        &service
            .read_presentation("{\"v\":4,\"req\":\"read-1\"}")
            .expect("in-band"),
    )
    .expect("valid JSON");
    assert_eq!(projection["outcome"], "presentation");
    assert!(
        !projection["preview"].is_null(),
        "moved open update previews, got {projection}"
    );

    // Held open (zero movement against the immutable START point) carries
    // no overlay: the cached preview clears to null instead of holding a
    // stale shape.
    let held = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "2",
            "updated",
            "win-a",
            "move",
            &ra,
            "[10,10]",
            "[10,10]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 2, &held);
    let projection: serde_json::Value = serde_json::from_str(
        &service
            .read_presentation("{\"v\":4,\"req\":\"read-2\"}")
            .expect("in-band"),
    )
    .expect("valid JSON");
    assert!(
        projection["preview"].is_null(),
        "held open clears the stale preview, got {projection}"
    );

    // First terminal with frozen start facts settles exactly once.
    let terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "3",
            "released",
            "win-a",
            "move",
            &ra,
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 3, &terminal);
    assert_eq!(
        settled["gestureSettled"],
        serde_json::json!({"id": "1", "committed": true}),
        "first terminal settles with frozen start facts"
    );

    // Re-sending the consumed terminal on a fresh source stays dead: no
    // second commit, retained settlement and association remain unchanged.
    let replay = capture(&service, &generation, 4, &terminal);
    assert_eq!(
        replay["gestureSettled"], settled["gestureSettled"],
        "consumed terminal retains original settlement unchanged, got {replay}"
    );
    assert_eq!(
        replay["gestureSettledAssoc"], settled["gestureSettledAssoc"],
        "consumed terminal retains original settled association unchanged, got {replay}"
    );
}

#[test]
fn unavailable_and_superseded_omit_without_map_reset() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();

    let open1 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    let replied = capture(&service, &generation, 1, &open1);
    assert_eq!(replied["outcome"], serde_json::Value::Null);

    // Explicit effect teardown omits truthfully: accepted, no settlement.
    let gone = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "0",
            "2",
            "idle",
            "",
            "",
            "null",
            "null",
            "null",
            0,
            1,
            0,
            0,
        ),
        "null",
        "null",
    );
    let omitted = capture(&service, &generation, 2, &gone);
    assert_eq!(omitted["outcome"], serde_json::Value::Null);
    assert!(omitted["gestureSettled"].is_null());

    // A superseded waiter likewise omits without touching the map.
    let superseded = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "2",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            1,
            1,
        ),
        "null",
        "null",
    );
    let stood_down = capture(&service, &generation, 3, &superseded);
    assert_eq!(stood_down["outcome"], serde_json::Value::Null);
    assert!(stood_down["gestureSettled"].is_null());

    // The mapping persists across the unavailable interval: the next
    // serial continues the same broker namespace (core id 2).
    let open2 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("2", "3", "[150,50]"),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 4, &open2);
    let moved_a = "[100,0,200,200]".to_owned();
    let terminal2 = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("2", "4", "[150,50]"),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 5, &terminal2);
    assert_eq!(
        settled["gestureSettled"],
        serde_json::json!({"id": "2", "committed": true}),
        "map persists across unavailable intervals, got {settled}"
    );
}

#[test]
fn fast_cancelled_and_consumed_esc_released_pass_verbatim() {
    let service = WorldService::new();
    let generation = discover(&service);
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    // Fast START-Esc-finish resolves upstream to Cancelled: a moved frame
    // still never commits.
    let cancelled = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "1",
            "cancelled",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 1, &cancelled);
    assert_eq!(
        settled["gestureSettled"],
        serde_json::json!({"id": "1", "committed": false}),
        "cancelled passes verbatim without commit, got {settled}"
    );

    // A consumed Esc finishes as Released upstream and the broker preserves
    // the phase verbatim (never a Cancelled rewrite): the moved frame
    // commits.
    let released = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "2",
            "2",
            "released",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let committed = capture(&service, &generation, 2, &released);
    assert_eq!(
        committed["gestureSettled"],
        serde_json::json!({"id": "2", "committed": true}),
        "released passes verbatim with commit, got {committed}"
    );
}

#[test]
fn invalid_script_facts_read_unknown_without_refusal_or_invention() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();

    // An unreadable script cursor degrades the open fold to unknown (no
    // preview from a null cursor), never a refusal.
    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "1",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "null",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let replied = capture(&service, &generation, 1, &open);
    assert_eq!(replied["outcome"], serde_json::Value::Null);
    let projection = preview_of(&service, "read-invalid-1");
    assert!(
        projection["preview"].is_null(),
        "invalid script cursor reads unknown, got {projection}"
    );

    // An invalid windowAt fact never invents a target: the terminal still
    // settles with the native window identity.
    let moved_a = "[100,0,200,200]".to_owned();
    let terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "2", "[150,50]"),
        "null",
        "\"not a window id!!\"",
    );
    let settled = capture(&service, &generation, 2, &terminal);
    assert_eq!(
        settled["gestureSettled"],
        serde_json::json!({"id": "1", "committed": true}),
        "invalid windowAt never invents a target, got {settled}"
    );

    // A well-formed windowAt naming another window still never overrides
    // the native gesture target.
    let open_other = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "2",
            "3",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "\"win-b\"",
    );
    let opened_other = capture(&service, &generation, 3, &open_other);
    assert_eq!(opened_other["outcome"], serde_json::Value::Null);
    let terminal_other = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "2",
            "4",
            "released",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "\"win-b\"",
    );
    let committed = capture(&service, &generation, 4, &terminal_other);
    assert_eq!(
        committed["gestureSettled"]["id"],
        serde_json::json!("2"),
        "windowAt never overrides the native target, got {committed}"
    );
}

#[test]
fn identity_map_stays_bounded_without_replay() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();
    let mut seq: u64 = 0;
    let mut next = || {
        seq += 1;
        seq
    };

    // Hundreds of distinct gestures keep working past any bounded map: the
    // latest terminal settles with its own fresh core id.
    for serial in 1u64..=300 {
        let open = with_raw(
            &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
            &raw_native_full(
                "gesture-1",
                &serial.to_string(),
                &serial.to_string(),
                "updated",
                "win-a",
                "move",
                "[0,0,200,200]",
                "[10,10]",
                "[150,50]",
                1,
                1,
                0,
                1,
            ),
            "null",
            "null",
        );
        let replied = capture(&service, &generation, next(), &open);
        assert_eq!(
            replied["outcome"],
            serde_json::Value::Null,
            "serial {serial} opens"
        );
        let terminal = with_raw(
            &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
            &raw_native_full(
                "gesture-1",
                &serial.to_string(),
                &format!("{}", serial + 100_000),
                "released",
                "win-a",
                "move",
                "[0,0,200,200]",
                "[10,10]",
                "[150,50]",
                1,
                1,
                0,
                1,
            ),
            "null",
            "null",
        );
        let settled = capture(&service, &generation, next(), &terminal);
        assert_eq!(
            settled["gestureSettled"]["id"],
            serde_json::json!(serial.to_string()),
            "serial {serial} settles with its own id"
        );
    }

    // An ancient evicted identity replayed under a fresh source stays dead:
    // bounded retirement never rebinds it to a newer core id and retains
    // serial 300's settlement.
    let ancient = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "999999", "[150,50]"),
        "null",
        "null",
    );
    let replayed = capture(&service, &generation, next(), &ancient);
    assert!(
        replayed["gestureAssoc"].is_null(),
        "evicted identities omit ingress, got {replayed}"
    );
    assert_eq!(
        replayed["gestureSettled"],
        serde_json::json!({"id": "300", "committed": false}),
        "evicted identities retain last settled terminal, got {replayed}"
    );
    assert_eq!(
        replayed["gestureSettledAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "300", "coreId": "300"}),
        "evicted identities retain last settled association, got {replayed}"
    );
}

#[test]
fn exact_envelope_replay_recovers_byte_identical_settled_response() {
    // Processed-reply-lost: the broker processed the terminal but the
    // actuator never saw the reply. Redelivering the byte-identical
    // capture envelope replays the remembered bound response (same
    // settlement and same bound association) without re-executing the
    // core, so the stranded terminal recovers with no double commit.
    // Fresh-source re-execution alone cannot recover it: the consumed
    // terminal omits on a new source.
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();
    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 1, &open);
    let terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "2", "[150,50]"),
        "null",
        "null",
    );
    let envelope = capture_envelope_text(&generation, 2, &terminal);
    let first = service.evaluate_capture(&envelope).expect("in-band");
    let settled: serde_json::Value = serde_json::from_str(&first).expect("reply is JSON");
    assert_eq!(settled["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(
        settled["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"}),
    );
    // Byte-identical redelivery replays exactly (no re-execution).
    let second = service.evaluate_capture(&envelope).expect("in-band");
    assert_eq!(second, first, "exact replay recovers the settled response");
    let replayed: serde_json::Value = serde_json::from_str(&second).expect("reply is JSON");
    assert_eq!(replayed["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(replayed["gestureAssoc"], settled["gestureAssoc"]);
    // Fresh-source re-execution of the consumed terminal does not re-settle
    // or mint a newer id; the retained terminal and association remain unchanged.
    let fresh = capture(&service, &generation, 3, &terminal);
    assert_eq!(
        fresh["gestureSettled"], settled["gestureSettled"],
        "consumed terminal retains prior settlement on a fresh source, got {fresh}"
    );
    assert_eq!(
        fresh["gestureSettledAssoc"], settled["gestureSettledAssoc"],
        "consumed terminal retains prior settled association on a fresh source, got {fresh}"
    );
}

#[test]
fn staged_map_discards_malformed_report_without_spending_identity() {
    // D: a malformed non-gesture report field beside a valid native lane
    // must not retire sources or spend identity/revision. The staged
    // gesture map installs only with an accepted reply.
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    let valid_open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    // Same valid native lane plus a malformed unknown top-level field: the
    // strict decode refuses after gesture normalization ran on the stage.
    let malformed = valid_open.replacen(
        "\"rendererInput\":null",
        "\"rendererInput\":null,\"bogusField\":1",
        1,
    );
    let refused = capture(&service, &generation, 1, &malformed);
    assert_eq!(refused["outcome"], "rejected");

    // The valid old source still maps to the first core id (never spent):
    // accepted with the proven bound association.
    let retry = capture(&service, &generation, 2, &valid_open);
    assert_eq!(retry["outcome"], serde_json::Value::Null);
    assert_eq!(
        retry["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"}),
        "staged map discarded the malformed mutation, got {retry}"
    );
    let terminal = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "2", "[150,50]"),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 3, &terminal);
    assert_eq!(settled["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(
        settled["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"}),
        "terminal carries the bound association, got {settled}"
    );
}

#[test]
fn open_prefers_script_cursor_over_native() {
    // E: open snapshots prefer the same-capture readable script cursor and
    // fall back to native only when the script point is unknown. Native at
    // the zero gate with a moved script pointer still previews (script
    // wins); terminal freezes native regardless of script.
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "1",
            "updated",
            "win-a",
            "move",
            "[0,0,200,200]",
            "[10,10]",
            "[10,10]",
            1,
            1,
            0,
            1,
        ),
        "[150,50]",
        "null",
    );
    let replied = capture(&service, &generation, 1, &open);
    assert_eq!(replied["outcome"], serde_json::Value::Null);
    assert_eq!(
        replied["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"}),
    );
    let projection = preview_of(&service, "read-script-wins");
    assert!(
        !projection["preview"].is_null(),
        "script cursor wins over native zero gate, got {projection}"
    );
}

#[test]
fn bound_assoc_absent_on_omitted_and_proves_settled_binding() {
    // F: the broker-only bound association rides accepted replies with a
    // normalized ingress and is absent (null) on omitted lanes. An
    // unrelated settled id (retained close) never proves the current
    // ingress.
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    // Omitted lane (stale serial below floor after a newer serial mapped):
    // accepted with null current-ingress association while retaining the
    // prior terminal and its settled association.
    let open2 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("2", "1", "[150,50]"),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 1, &open2);
    let terminal2 = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("2", "2", "[150,50]"),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 2, &terminal2);
    assert_eq!(settled["gestureSettled"]["id"], serde_json::json!("1"));
    let stale = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "3", "[150,50]"),
        "null",
        "null",
    );
    let omitted = capture(&service, &generation, 3, &stale);
    assert_eq!(omitted["outcome"], serde_json::Value::Null);
    assert_eq!(omitted["gestureSettled"], settled["gestureSettled"]);
    assert_eq!(
        omitted["gestureSettledAssoc"],
        settled["gestureSettledAssoc"]
    );
    assert!(
        omitted["gestureAssoc"].is_null(),
        "omitted lanes carry no association, got {omitted}"
    );

    // Open accepted reply carries the association for the next package while
    // retaining the prior completed settlement until the successor settles.
    let open3 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("3", "4", "[150,50]"),
        "null",
        "null",
    );
    let opened = capture(&service, &generation, 4, &open3);
    assert_eq!(opened["outcome"], serde_json::Value::Null);
    assert_eq!(opened["gestureSettled"], settled["gestureSettled"]);
    assert_eq!(
        opened["gestureSettledAssoc"],
        settled["gestureSettledAssoc"]
    );
    assert_eq!(
        opened["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "3", "coreId": "2"}),
        "open carries proven binding without terminal, got {opened}"
    );
}

#[test]
fn gesture_settled_assoc_retains_terminal_distinct_from_current_ingress_and_resets_on_handshake() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    let open1 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    let opened1 = capture(&service, &generation, 1, &open1);
    assert_eq!(
        opened1["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"}),
    );
    assert!(opened1["gestureSettledAssoc"].is_null());

    let terminal1 = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "2", "[150,50]"),
        "null",
        "null",
    );
    let settled1 = capture(&service, &generation, 2, &terminal1);
    let assoc1 = serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"});
    assert_eq!(settled1["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(settled1["gestureAssoc"], assoc1);
    assert_eq!(settled1["gestureSettledAssoc"], assoc1);

    // Idle capture: gestureAssoc is null while gestureSettledAssoc retains terminal 1.
    let idle = inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]);
    let idle_reply = capture(&service, &generation, 3, &idle);
    assert!(idle_reply["gestureAssoc"].is_null());
    assert_eq!(idle_reply["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(idle_reply["gestureSettledAssoc"], assoc1);

    // Opening successor gesture 2 keeps terminal 1's gestureSettledAssoc distinct
    // from current-ingress gestureAssoc (Q1 A).
    let open2 = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_open("2", "1", "[150,50]"),
        "null",
        "null",
    );
    let opened2 = capture(&service, &generation, 4, &open2);
    assert_eq!(
        opened2["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "2", "coreId": "2"}),
    );
    assert_eq!(opened2["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(opened2["gestureSettledAssoc"], assoc1);

    // Fresh handshake resets both active and terminal associations.
    let generation2 = discover(&service);
    let after_reset = capture(&service, &generation2, 1, &idle);
    assert!(after_reset["gestureAssoc"].is_null());
    assert!(after_reset["gestureSettled"].is_null());
    assert!(after_reset["gestureSettledAssoc"].is_null());
}

#[test]
fn broker_rejections_and_connection_errors_carry_null_gesture_settled_assoc() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();
    let moved_a = "[100,0,200,200]".to_owned();

    // Connection-error envelope carries null gestureAssoc and null gestureSettledAssoc.
    let err_env: serde_json::Value =
        serde_json::from_str(&service.error_body("malformed", "request is malformed"))
            .expect("error JSON");
    assert_eq!(err_env["outcome"], "rejected");
    assert!(err_env["gestureAssoc"].is_null());
    assert!(err_env["gestureSettledAssoc"].is_null());

    // Settle a gesture first so terminal_assoc is populated.
    let open1 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    let _ = capture(&service, &generation, 1, &open1);
    let terminal1 = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "2", "[150,50]"),
        "null",
        "null",
    );
    let settled = capture(&service, &generation, 2, &terminal1);
    let assoc1 = serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"});
    assert_eq!(settled["gestureSettledAssoc"], assoc1);

    // Generation-mismatch rejection carries null gestureSettledAssoc.
    let wrong_gen = capture(&service, "wrong-gen", 3, &terminal1);
    assert_eq!(wrong_gen["outcome"], "rejected");
    assert!(wrong_gen["gestureAssoc"].is_null());
    assert!(wrong_gen["gestureSettledAssoc"].is_null());

    // Malformed capture rejection carries null gestureSettledAssoc and does
    // not disturb the pinned terminal association.
    let malformed = terminal1.replacen(
        "\"rendererInput\":null",
        "\"rendererInput\":null,\"bogusField\":1",
        1,
    );
    let refused = capture(&service, &generation, 3, &malformed);
    assert_eq!(refused["outcome"], "rejected");
    assert!(refused["gestureAssoc"].is_null());
    assert!(refused["gestureSettledAssoc"].is_null());

    // Subsequent accepted idle capture still carries the pinned terminal association.
    let idle = inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]);
    let idle_reply = capture(&service, &generation, 4, &idle);
    assert_eq!(idle_reply["gestureSettledAssoc"], assoc1);
}

#[test]
fn close_with_omitted_ingress_resolves_old_active_terminal_after_256_unbindable_newcomers() {
    let service = WorldService::new();
    let generation = discover(&service);
    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();

    let open1 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("1", "1", "[150,50]"),
        "null",
        "null",
    );
    let opened1 = capture(&service, &generation, 1, &open1);
    let assoc1 = serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"});
    assert_eq!(opened1["gestureAssoc"], assoc1);

    // 256 unbindable newcomers (unknown window) evict (gesture-1, "1") from
    // the 256-entry normalization map without replacing core's active gesture.
    for i in 0..256u64 {
        let serial = format!("{}", 2 + i);
        let newcomer = with_raw(
            &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
            &raw_native_full(
                "gesture-1",
                &serial,
                "1",
                "started",
                "win-missing",
                "move",
                "[0,0,200,200]",
                "[10,10]",
                "[150,50]",
                1,
                1,
                0,
                1,
            ),
            "null",
            "null",
        );
        let reply = capture(&service, &generation, 2 + i, &newcomer);
        assert_eq!(reply["outcome"], serde_json::Value::Null);
    }

    // Close win-a with omitted ingress: core settles active gesture 1
    // (committed = false), and broker resolves gestureSettledAssoc from the
    // pinned active association.
    let closed_inner = inner_report(&[window_json("win-b", &rb)]).replacen(
        "\"closed\":[]",
        "\"closed\":[\"win-a\"]",
        1,
    );
    let closed_reply = capture(&service, &generation, 300, &closed_inner);
    assert!(closed_reply["gestureAssoc"].is_null());
    assert_eq!(closed_reply["gestureSettled"]["id"], serde_json::json!("1"));
    assert_eq!(
        closed_reply["gestureSettled"]["committed"],
        serde_json::json!(false)
    );
    assert_eq!(closed_reply["gestureSettledAssoc"], assoc1);
}

#[test]
fn retained_preview_stays_proven_across_omitted_ingress_newcomers_and_resets_on_handshake() {
    let service = WorldService::new();
    let discovery: serde_json::Value =
        serde_json::from_str(&handshake_discover(&service, ADAPTER, "req-0")).expect("valid JSON");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    apply_settings(&service, &generation);

    let ra = "[0,0,200,200]".to_owned();
    let rb = "[300,0,200,200]".to_owned();

    // Open update with moved cursor produces a proven dragPreview.
    let open = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_native_full(
            "gesture-1",
            "1",
            "1",
            "updated",
            "win-a",
            "move",
            &ra,
            "[10,10]",
            "[150,50]",
            1,
            1,
            0,
            1,
        ),
        "null",
        "null",
    );
    let reply1 = capture(&service, &generation, 1, &open);
    assert!(reply1.get("result").is_none());
    assert!(reply1["dragPreview"].is_object());
    let proj1 = preview_of(&service, "read-1");
    assert_eq!(proj1["preview"]["producerInstance"], "gesture-1");
    assert_eq!(proj1["preview"]["producerSerial"], "1");

    // Omitted ingress retains both dragPreview and its proven producer identity.
    let omitted = inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]);
    let reply2 = capture(&service, &generation, 2, &omitted);
    assert!(reply2["gestureAssoc"].is_null());
    assert_eq!(reply2["dragPreview"], reply1["dragPreview"]);
    let proj2 = preview_of(&service, "read-2");
    assert_eq!(proj2["preview"], proj1["preview"]);

    // 256 unbindable newcomers (unknown window) evict (gesture-1, "1") from
    // by_key, yet cannot relabel the retained preview or drop its pinned proof.
    for i in 0..256u64 {
        let serial = format!("{}", 2 + i);
        let newcomer = with_raw(
            &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
            &raw_native_full(
                "gesture-1",
                &serial,
                "1",
                "started",
                "win-missing",
                "move",
                &ra,
                "[10,10]",
                "[150,50]",
                1,
                1,
                0,
                1,
            ),
            "null",
            "null",
        );
        let _ = capture(&service, &generation, 3 + i, &newcomer);
    }
    let proj_after = preview_of(&service, "read-after-evict");
    assert_eq!(proj_after["preview"]["producerInstance"], "gesture-1");
    assert_eq!(proj_after["preview"]["producerSerial"], "1");

    // Same-generation settings/read continuity preserves the retained preview.
    apply_settings(&service, &generation);
    let proj_same_gen = preview_of(&service, "read-same-gen");
    assert_eq!(proj_same_gen["preview"], proj_after["preview"]);

    // Fresh handshake resets preview and pins.
    let discovery2: serde_json::Value =
        serde_json::from_str(&handshake_discover(&service, ADAPTER, "req-1")).expect("valid JSON");
    let gen2 = discovery2["gen"].as_str().expect("gen2").to_owned();
    assert_ne!(gen2, generation);
    let reset_read: serde_json::Value = serde_json::from_str(
        &service
            .read_presentation("{\"v\":4,\"req\":\"read-reset\"}")
            .expect("in-band"),
    )
    .expect("valid JSON");
    assert_eq!(reset_read["outcome"], "rejected");
    assert_eq!(reset_read["kind"], "not-ready");
}
