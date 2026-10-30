//! Broker-path gesture lifecycle transport: the production WorldService
//! capture lane folds the effect-owned raw native lane (snapshot plus
//! same-capture script cursor/windowAt facts) into core ingress in the
//! broker, the first terminal with frozen start facts settles exactly
//! once, replays stay dead, and a held open clears a stale drop preview.

use omnitiler::world_service::WorldService;

const ADAPTER: &str = "kwin-actuator-1";

fn window_json(id: &str, rect: &str) -> String {
    let frame = rect
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"frameGeometry\":{{\"x\":{x},\"y\":{y},\"width\":{w},\"height\":{h}}},\
          \"clientGeometry\":{{\"x\":{x},\"y\":{y},\"width\":{w},\"height\":{h}}},\
          \"minSize\":null,\"maxSize\":null,\"desktops\":[],\
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

fn inner_report(windows: &[String]) -> String {
    format!(
        "{{\"v\":3,\"mode\":\"per-output-local\",\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"gap\":8,\"outer\":4,\"complete\":true,\"primary\":true}}],\
          \"windows\":[{}],\"views\":[{{\"out\":\"DP-1\",\"view\":\"\"}}],\"closed\":[],\
          \"hostOwned\":false,\"equalCount\":false,\"birthClear\":true,\"native\":[],\
          \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
          \"bornMaximized\":\"tile-on-launch\",\
          \"defaultTiled\":true,\"focused\":\"\",\
          \"complete\":true,\"gesture\":true,\"switchTarget\":\"cursor-output\",\
          \"freshPlacement\":\"cursor-output-last-focused\",\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\"focusFollowsCursor\":false,\"cursorWarp\":false,\
          \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
          \"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\
          \"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\"borderGap\":0,\
          \"borderWidth\":0,\"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\
          \"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}},\
          \"creationAttempts\":[],\"frameAttempts\":[],\"memberAttempts\":[],\
          \"viewAttempts\":[],\"clearAttempts\":[],\
          \"floatIntent\":[],\"tileIntent\":[],\
          \"anchorFallback\":\"keep\",\"noSpaceResult\":\"keep\"}}",
        windows.join(","),
    )
}

fn with_ingress(inner: &str, ingress: &str) -> String {
    inner.replacen(
        ",\"anchorFallback\"",
        &format!(",\"gestureIngress\":{ingress},\"anchorFallback\""),
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
        "{{\"v\":3,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
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
        ",\"anchorFallback\"",
        &format!(
            ",\"gestureIngress\":null,\"nativeGesture\":{native},\
             \"cursorWindow\":{cursor_window},\"anchorFallback\""
        ),
        1,
    );
    stepped.replacen("\"cursor\":null", &format!("\"cursor\":{cursor}"), 1)
}

fn discover(service: &WorldService) -> String {
    let discovery: serde_json::Value =
        serde_json::from_str(&service.discovery_body(ADAPTER, "req-0")).expect("valid JSON");
    discovery["gen"].as_str().expect("gen").to_owned()
}

fn preview_of(service: &WorldService, req: &str) -> serde_json::Value {
    serde_json::from_str(
        &service
            .read_presentation(&format!("{{\"v\":3,\"req\":\"{req}\"}}"))
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
    // terminal omits truthfully and settles nothing.
    let stale = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "3", "[150,50]"),
        "null",
        "null",
    );
    let omitted = capture(&service, &generation, 3, &stale);
    assert_eq!(omitted["outcome"], serde_json::Value::Null);
    assert!(
        omitted["gestureSettled"].is_null(),
        "stale serial settles nothing, got {omitted}"
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
        omitted_terminal["gestureSettled"].is_null(),
        "retired terminal settles nothing, got {omitted_terminal}"
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
        )
        .replacen(",\"gestureIngress\":null", "", 1),
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
        serde_json::from_str(&service.discovery_body(ADAPTER, "req-0")).expect("valid JSON");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();

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
            .read_presentation("{\"v\":3,\"req\":\"read-1\"}")
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
            .read_presentation("{\"v\":3,\"req\":\"read-2\"}")
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

    // Exact replay of the consumed terminal stays dead: no second commit.
    let replay = capture(&service, &generation, 4, &terminal);
    assert!(
        replay["gestureSettled"].is_null(),
        "consumed terminal replays dead, got {replay}"
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
    // bounded retirement never rebinds it to a newer core id.
    let ancient = with_raw(
        &inner_report(&[window_json("win-a", &moved_a), window_json("win-b", &rb)]),
        &raw_terminal("1", "999999", "[150,50]"),
        "null",
        "null",
    );
    let replayed = capture(&service, &generation, next(), &ancient);
    assert!(
        replayed["gestureSettled"].is_null(),
        "evicted identities never rebind, got {replayed}"
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
    // Fresh-source re-execution of the consumed terminal settles nothing.
    let fresh = capture(&service, &generation, 3, &terminal);
    assert!(
        fresh["gestureSettled"].is_null(),
        "consumed terminal never re-settles on a fresh source, got {fresh}"
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
        "\"anchorFallback\":\"keep\"",
        "\"anchorFallback\":\"keep\",\"bogusField\":1",
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
    // accepted with null settlement AND null association.
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
    assert!(omitted["gestureSettled"].is_null());
    assert!(
        omitted["gestureAssoc"].is_null(),
        "omitted lanes carry no association, got {omitted}"
    );

    // Open accepted reply carries the association for the next package even
    // with no terminal settlement.
    let open3 = with_raw(
        &inner_report(&[window_json("win-a", &ra), window_json("win-b", &rb)]),
        &raw_open("3", "4", "[150,50]"),
        "null",
        "null",
    );
    let opened = capture(&service, &generation, 4, &open3);
    assert_eq!(opened["outcome"], serde_json::Value::Null);
    assert!(opened["gestureSettled"].is_null());
    assert_eq!(
        opened["gestureAssoc"],
        serde_json::json!({"instance": "gesture-1", "serial": "3", "coreId": "2"}),
        "open carries proven binding without terminal, got {opened}"
    );
}
