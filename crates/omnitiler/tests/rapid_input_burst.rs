//! KDE rapid-input broker regressions (change record
//! `kde-rapid-input-latest-world`): queue ONLY parks plus `WorldWake`; ONE
//! fresh capture incorporates native observations once then drains ALL parked
//! commands FIFO against post-previous-command state with ONE final full
//! publication. No queue-time execution, no command-only publication lane.
//! Commands arriving during/after a capture park for the next batch.
//! `win:{"currentFocus":true}` stays the execution-time core-focus spelling; explicit by-id
//! subjects and the empty `focus-directional` no-focus keep their meanings.

use omnitiler::world_service::WorldService;

const SENDER_A: &str = ":1.42";
const ADAPTER: &str = "kwin-actuator-1";

fn disabled_presentation() -> String {
    "{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
     \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\
     \"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\
     \"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\
     \"foregroundOut\":null}"
        .to_owned()
}

fn window_json(id: &str) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"rect\":[0,0,400,300],\
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
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}}}}"
    )
}

fn window_json_native(id: &str) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"rect\":[0,0,400,300],\
          \"nativeWs\":\"n-0\",\
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
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}}}}"
    )
}

/// Minimal v4 inner report: one complete screen, caller-chosen windows and
/// explicit closes. Strict v4 ingress; the broker stamps the sequencing
/// namespace.
fn inner_report_closed(windows: &[String], closed: &[String]) -> String {
    inner_report_closed_focused(windows, closed, "")
}

/// Minimal v4 inner report with caller-chosen staged native focus.
fn inner_report_closed_focused(windows: &[String], closed: &[String], focused: &str) -> String {
    format!(
        "{{\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[{}],\"native\":[],\
          \"focused\":\"{focused}\",\"foregroundKnown\":{},\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{},\
          \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\
          \"rendererInput\":null}}",
        windows.join(","),
        closed
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(","),
        !focused.is_empty(),
        disabled_presentation()
    )
}

/// v4 inner report carrying valid host-owned native rows: one ordered
/// native desktop observed on DP-1. The bound handshake advertises
/// `hostOwned:true`, so this evidence is admission-valid.
fn inner_report_host_native(windows: &[String], focused: &str) -> String {
    format!(
        "{{\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[],\
          \"native\":[{{\"out\":\"DP-1\",\"order\":[\"n-0\"],\
          \"current\":\"n-0\",\"occupancy\":[{{\"native\":\"n-0\",\"count\":1}}]}}],\
          \"focused\":\"{focused}\",\"foregroundKnown\":{},\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{},\
          \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\
          \"rendererInput\":null}}",
        windows.join(","),
        !focused.is_empty(),
        disabled_presentation()
    )
}

fn capture(adapter: &str, generation: &str, seq: u64, watermark: u64, inner: &str) -> String {
    capture_source(adapter, generation, &js_source(seq), seq, watermark, inner)
}

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

fn handshake_req_owned(adapter: &str, req: &str, host_owned: bool) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"fixture-actuator\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":{host_owned},\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
          \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
          \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
          \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
    )
}

/// Window fact at a caller-chosen x origin: directional neighbors need
/// distinct frames, while the shared fields keep the established valid
/// shape.
fn window_json_at(id: &str, x: i32) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"rect\":[{x},0,400,300],\
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
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}}}}"
    )
}

fn settings_req(adapter: &str, generation: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\
          \"gen\":\"{generation}\",\"req\":\"{req}\",\"rev\":\"1\",\
          \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\
          \"bornMaximized\":\"tile-on-launch\"}}}}"
    )
}

fn queue_req(generation: &str, client: &str, client_cmd: &str, command: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"{client}\",\
          \"clientCmd\":\"{client_cmd}\",\"command\":{command}}}"
    )
}

fn take_req(generation: &str, client: &str, client_cmd: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"{client}\",\
          \"clientCmd\":\"{client_cmd}\"}}"
    )
}

fn discovered_service() -> (WorldService, String) {
    discovered_service_owned(false)
}

fn discovered_service_owned(host_owned: bool) -> (WorldService, String) {
    let service = WorldService::new();
    let discovery: serde_json::Value = parse(
        &service
            .discover(&handshake_req_owned(ADAPTER, "req-0", host_owned))
            .expect("handshake routes"),
    );
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    assert!(!generation.is_empty());
    let settings: serde_json::Value = parse(
        &service
            .discover(&settings_req(ADAPTER, &generation, "settings-0"))
            .expect("settings routes"),
    );
    assert_eq!(settings["status"], "accepted");
    (service, generation)
}

fn file_command(service: &WorldService, generation: &str, id: &str, command: &str) {
    let filed = parse(
        &service
            .queue_command(SENDER_A, &queue_req(generation, "panel-1", id, command))
            .expect("in-band"),
    );
    assert_eq!(filed["outcome"], "queued", "filing {id} parks");
}

fn take_outcome(service: &WorldService, generation: &str, id: &str) -> serde_json::Value {
    parse(
        &service
            .take_result(SENDER_A, &take_req(generation, "panel-1", id))
            .expect("in-band"),
    )
}

fn focus_cmd(window: &str) -> String {
    format!("{{\"op\":\"focus\",\"win\":\"{window}\"}}")
}

#[test]
fn rapid_input_queued_burst_parks_until_single_capture_drains_all() {
    // Queue parks only: every take is pending until ONE fresh capture
    // arrives, which drains ALL parked commands FIFO with one publication.
    let (service, generation) = discovered_service();
    let inner = inner_report_closed(&[], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("retained state established");
    for id in ["1", "2", "3"] {
        file_command(
            &service,
            &generation,
            id,
            "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
        );
        assert_eq!(
            take_outcome(&service, &generation, id)["outcome"],
            "pending",
            "command {id} parks until capture"
        );
    }
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("single fresh capture drains"),
    );
    assert_eq!(reply["rev"], "2", "one capture advances one revision");
    for id in ["1", "2", "3"] {
        assert_eq!(
            take_outcome(&service, &generation, id)["outcome"],
            "ready",
            "burst command {id} settles from the single capture"
        );
    }
}

#[test]
fn rapid_input_single_action_parks_without_capture_execution() {
    // No queue-time execution: filing alone never settles the take lane.
    let (service, generation) = discovered_service();
    let inner = inner_report_closed(&[], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("retained state established");
    file_command(
        &service,
        &generation,
        "1",
        "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
    );
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "pending",
        "queued action must not execute without a capture"
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
        .expect("capture drains");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "capture settles the parked action"
    );
}

#[test]
fn rapid_input_current_focus_subject_resolves_from_prior_focus_in_batch() {
    // `win:{"currentFocus":true}` resolves from core focus after the preceding action in
    // the same batch: focus first records the subject, the current-focus
    // maximize then executes against it, all from one capture.
    let (service, generation) = discovered_service();
    let admit =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(&service, &generation, "1", &focus_cmd("win-a"));
    file_command(
        &service,
        &generation,
        "2",
        "{\"op\":\"toggle-maximize\",\"win\":{\"currentFocus\":true}}",
    );
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "pending"
    );
    assert_eq!(
        take_outcome(&service, &generation, "2")["outcome"],
        "pending"
    );
    let steady =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &steady))
        .expect("batch capture drains");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "explicit focus settles"
    );
    assert_eq!(
        take_outcome(&service, &generation, "2")["outcome"],
        "ready",
        "current-focus maximize resolves retained focus from the same batch"
    );
}

#[test]
fn rapid_input_unknown_subject_fails_without_wedging_batch() {
    // Per-command results are individual: an unknown by-id subject settles
    // failed while the rest of the FIFO batch still settles ready.
    let (service, generation) = discovered_service();
    let admit =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(&service, &generation, "1", &focus_cmd("win-b"));
    file_command(&service, &generation, "2", &focus_cmd("win-zz"));
    file_command(&service, &generation, "3", &focus_cmd("win-a"));
    let steady =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &steady))
        .expect("batch capture drains");
    assert_eq!(take_outcome(&service, &generation, "1")["outcome"], "ready");
    let failed = take_outcome(&service, &generation, "2");
    assert_eq!(
        failed["outcome"], "failed",
        "unknown subject settles failed"
    );
    assert_eq!(failed["kind"], "focus-unknown");
    assert_eq!(take_outcome(&service, &generation, "3")["outcome"], "ready");
}

#[test]
fn rapid_input_mixed_100_action_burst_single_capture() {
    // 100 mixed actions, no capture between filings: all park, one
    // capture settles every one, one revision bump.
    let (service, generation) = discovered_service();
    let admit =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    for n in 1..=100u64 {
        let id = n.to_string();
        let target = if n % 2 == 0 { "win-b" } else { "win-a" };
        file_command(&service, &generation, &id, &focus_cmd(target));
    }
    for n in 1..=100u64 {
        assert_eq!(
            take_outcome(&service, &generation, &n.to_string())["outcome"],
            "pending",
            "action {n} parks"
        );
    }
    let steady =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &steady))
            .expect("single capture drains 100"),
    );
    assert_eq!(reply["rev"], "2", "100 actions share one revision");
    for n in 1..=100u64 {
        assert_eq!(
            take_outcome(&service, &generation, &n.to_string())["outcome"],
            "ready",
            "action {n} settles from the single capture"
        );
    }
}

#[test]
fn rapid_input_close_observation_amid_burst_not_lost() {
    // Close is retained lifecycle evidence: a queued command drains on the
    // close-carrying capture, and the close itself retires the member, so
    // a later by-id command at the closed window settles failed (unknown),
    // never revives it.
    let (service, generation) = discovered_service();
    let admit = inner_report_closed(&[window_json("win-a"), window_json("win-b")], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(
        &service,
        &generation,
        "1",
        "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
    );
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "pending"
    );
    let closing = inner_report_closed(&[window_json("win-a")], &["win-b".to_owned()]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &closing))
        .expect("close-carrying capture accepted");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "burst command drains on the close capture"
    );
    file_command(&service, &generation, "2", &focus_cmd("win-b"));
    let steady = inner_report_closed(&[window_json("win-a")], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &steady))
        .expect("follow-up capture accepted");
    let second = take_outcome(&service, &generation, "2");
    assert_eq!(
        second["outcome"], "failed",
        "focus at the closed window must settle failed (close retained); got {second}"
    );
}

#[test]
fn rapid_input_refused_capture_leaves_queue_parked() {
    // All-capture refusal adopts nothing and settles nothing: the queue
    // stays parked for the next valid capture.
    let (service, generation) = discovered_service();
    let inner = inner_report_closed(&[], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("retained state established");
    file_command(
        &service,
        &generation,
        "1",
        "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
    );
    // Unknown-field envelope refuses before dispatch. The retry must be a
    // NEW capture (advanced source sequence): redelivering the same source
    // with different bytes correctly refuses as source-reused, so the
    // next valid capture drains the still-parked queue.
    let mut bad: serde_json::Value =
        serde_json::from_str(&capture(ADAPTER, &generation, 2, 2, &inner)).expect("envelope");
    bad["report"]["bogus"] = serde_json::Value::Bool(true);
    let refused = parse(
        &service
            .evaluate_capture(&bad.to_string())
            .expect("in-band refusal"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "pending",
        "refused capture must not settle the parked command"
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
        .expect("valid capture drains");
    assert_eq!(take_outcome(&service, &generation, "1")["outcome"], "ready");
}

#[test]
fn rapid_input_exact_duplicate_capture_replays_bytes() {
    // Byte-identical redelivery replays exactly: no double execution, no
    // queue motion.
    let (service, generation) = discovered_service();
    let inner = inner_report_closed(&[], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("retained state established");
    file_command(
        &service,
        &generation,
        "1",
        "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
    );
    let envelope = capture(ADAPTER, &generation, 2, 2, &inner);
    let first = service.evaluate_capture(&envelope).expect("capture drains");
    let second = service
        .evaluate_capture(&envelope)
        .expect("duplicate replays");
    assert_eq!(first, second, "duplicate capture replays byte-identically");
    assert_eq!(take_outcome(&service, &generation, "1")["outcome"], "ready");
    // A second distinct capture advances exactly one more revision.
    let third = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
            .expect("next capture accepted"),
    );
    assert_eq!(
        parse(&first)["rev"],
        "2",
        "batch capture stamped exactly one revision"
    );
    assert_eq!(third["rev"], "3");
}

#[test]
fn rapid_input_host_native_evidence_joins_batch_once() {
    // Valid host-owned native rows ride the batch capture: the command
    // still settles and the native evidence incorporates without wedging.
    // Host ownership only admits native-bound rows, so the windows carry
    // their native workspace binding alongside the native desktop rows.
    let (service, generation) = discovered_service_owned(true);
    let admit = inner_report_host_native(
        &[window_json_native("win-a"), window_json_native("win-b")],
        "win-a",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("host-native admission accepted");
    file_command(&service, &generation, "1", &focus_cmd("win-b"));
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "pending"
    );
    let steady = inner_report_host_native(
        &[window_json_native("win-a"), window_json_native("win-b")],
        "win-a",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &steady))
        .expect("native-carrying batch capture accepted");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "command drains on the native-carrying capture"
    );
}

#[test]
fn rapid_input_pre_ready_queue_survives_until_first_usable_capture() {
    // Bootstrap: commands filed before the first usable capture must not
    // silently vanish; they drain on that first usable capture.
    let (service, generation) = discovered_service();
    file_command(
        &service,
        &generation,
        "1",
        "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
    );
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "pending",
        "pre-ready filing parks"
    );
    let inner = inner_report_closed(&[], &[]);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("first usable capture accepted");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "pre-ready command drains on the first usable capture"
    );
}

#[test]
fn rapid_input_empty_focus_directional_stays_no_focus() {
    // The explicit empty `focus-directional` subject keeps its genuine
    // no-focus meaning inside a batch: it settles without manufacturing a
    // focus proof for its neighbors.
    let (service, generation) = discovered_service();
    let admit =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(
        &service,
        &generation,
        "1",
        "{\"op\":\"focus-directional\",\"win\":\"\",\"direction\":\"right\"}",
    );
    file_command(&service, &generation, "2", &focus_cmd("win-b"));
    let steady =
        inner_report_closed_focused(&[window_json("win-a"), window_json("win-b")], &[], "win-a");
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &steady))
        .expect("batch capture drains");
    assert_eq!(take_outcome(&service, &generation, "1")["outcome"], "ready");
    assert_eq!(take_outcome(&service, &generation, "2")["outcome"], "ready");
}

fn implicit_focus_dir_cmd(direction: &str) -> String {
    format!(
        "{{\"op\":\"focus-directional\",\"win\":{{\"currentFocus\":true}},\
          \"direction\":\"{direction}\"}}"
    )
}

fn implicit_move_dir_cmd(direction: &str) -> String {
    format!(
        "{{\"op\":\"move-directional\",\"win\":{{\"currentFocus\":true}},\
          \"direction\":\"{direction}\",\"sameAxis\":\"group-with-neighbor\"}}"
    )
}

/// Admitted core neighbor topology prerequisite: the engine inserts
/// admitted clients into its own tiling order independent of the raw
/// native rect origins, so directional fixtures must prove the engine
/// plan before pressing a direction. Reads the setup capture reply's
/// published window rows (`alloc` falling back to `frame`).
fn assert_admitted_left_right(reply: &serde_json::Value, left: &str, right: &str) {
    let rows = reply["windows"]
        .as_array()
        .unwrap_or_else(|| panic!("topology prerequisite: reply has no windows array: {reply}"));
    let x_of = |window: &str| {
        let row = rows
            .iter()
            .find(|row| row["win"] == window)
            .unwrap_or_else(|| panic!("topology prerequisite: window {window} missing in {reply}"));
        let rect = if !row["alloc"].is_null() {
            &row["alloc"]
        } else {
            &row["frame"]
        };
        rect.as_array().unwrap_or_else(|| {
            panic!("topology prerequisite: window {window} has no published geometry: {row}")
        })[0]
            .as_i64()
            .unwrap_or_else(|| panic!("topology prerequisite: bad rect for {window}: {rect}"))
    };
    let lx = x_of(left);
    let rx = x_of(right);
    assert!(
        lx < rx,
        "topology prerequisite: {left} must sit left of {right} for left/right fixtures; got {left} x={lx} {right} x={rx}; reply={reply}"
    );
}

#[test]
fn rapid_input_two_implicit_focus_directionals_resolve_sequentially() {
    // Two execution-time focus directionals in ONE capture resolve
    // against post-previous-command core focus with ONE final full
    // publication. An executed setup focus establishes the slot and
    // the session leaf together; a commandless capture lets the native
    // fact catch up to the pre-batch focus. Right moves win-b to win-a,
    // left must resolve win-a from the first command and move back.
    let (service, generation) = discovered_service();
    let admit = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-a",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(&service, &generation, "1", &focus_cmd("win-b"));
    let setup = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-a",
    );
    let setup_reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &setup))
            .expect("setup focus executes"),
    );
    assert_admitted_left_right(&setup_reply, "win-b", "win-a");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "setup focus settles"
    );
    let catchup = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-b",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &catchup))
        .expect("native fact catches up");
    file_command(&service, &generation, "2", &implicit_focus_dir_cmd("right"));
    file_command(&service, &generation, "3", &implicit_focus_dir_cmd("left"));
    assert_eq!(
        take_outcome(&service, &generation, "2")["outcome"],
        "pending",
        "head parks until capture"
    );
    assert_eq!(
        take_outcome(&service, &generation, "3")["outcome"],
        "pending",
        "tail parks until capture"
    );
    let steady = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-b",
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 4, 4, &steady))
            .expect("batch capture drains"),
    );
    assert_eq!(reply["rev"], "4", "setup plus catch-up plus batch advance");
    let head = take_outcome(&service, &generation, "2");
    assert_eq!(head["outcome"], "ready", "head directional settles: {head}");
    let tail = take_outcome(&service, &generation, "3");
    assert_eq!(
        tail["outcome"], "ready",
        "tail directional resolves post-prior focus and settles: {tail}"
    );
}

#[test]
fn rapid_input_implicit_focus_then_move_uses_post_prior_subject() {
    // Mixed batch: the trailing implicit move resolves its subject from
    // the focus the preceding directional established (win-b), not the
    // stale pre-batch native focus (win-a). The stale win-a move-right
    // refuses against the post-head leaf while the sequential win-b
    // move succeeds.
    let (service, generation) = discovered_service();
    let admit = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-a",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(&service, &generation, "1", &focus_cmd("win-a"));
    let setup = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-a",
    );
    let setup_reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &setup))
            .expect("setup focus executes"),
    );
    assert_admitted_left_right(&setup_reply, "win-b", "win-a");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "setup focus settles"
    );
    file_command(&service, &generation, "2", &implicit_focus_dir_cmd("left"));
    file_command(&service, &generation, "3", &implicit_move_dir_cmd("right"));
    let steady = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-a",
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &steady))
            .expect("batch capture drains"),
    );
    assert_eq!(reply["rev"], "3", "setup plus batch advance");
    let head = take_outcome(&service, &generation, "2");
    assert_eq!(head["outcome"], "ready", "head directional settles: {head}");
    let tail = take_outcome(&service, &generation, "3");
    assert_eq!(
        tail["outcome"], "ready",
        "tail move resolves post-prior focus and settles: {tail}"
    );
}

#[test]
fn rapid_input_batch_saves_canonical_bytes_fit_reserve() {
    // Canonical-bytes reserve proof (not the structural estimator): the
    // batch reply the real service encoded round-trips through the public
    // canonical codec, and the exact `intentDesired` section - full encode
    // minus the empty-writes encode plus the 19-byte empty section - fits
    // the pre-mutation batch reserve. A second leg mutates the decoded
    // reply to 128 synthetic max-width rows and re-encodes through the
    // same public codec to prove the 128-row max-width bound without a
    // 128-window execution.
    use tiler_core::boundary::CoreCommand;
    use tiler_core::directional::WindowId;
    use tiler_core::intent_data::IntentData;
    use tiler_core::world_report::{IntentDesire, WorldCommand};
    use tiler_protocol::world_admission_sizing::{
        MAX_INTENT_DESIRED_ROW_BYTES, max_intent_save_batch_bound_v4,
    };
    use tiler_protocol::world_protocol::{decode_reply_v4, encode_reply_v4};

    let id_a = format!("a{:0127}", 1);
    let id_b = format!("b{:0127}", 2);
    assert_eq!(id_a.len(), 128, "max-width opaque subject");
    assert_eq!(id_b.len(), 128, "max-width opaque subject");
    let explicit_float =
        |window: &str| format!("{{\"op\":\"toggle-float\",\"win\":\"{window}\",\"rect\":null}}");
    const IMPLICIT_FLOAT: &str =
        "{\"op\":\"toggle-float\",\"win\":{\"currentFocus\":true},\"rect\":null}";

    let (service, generation) = discovered_service();
    let admit = inner_report_closed_focused(&[window_json(&id_a), window_json(&id_b)], &[], &id_a);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &admit))
        .expect("admission capture accepted");
    file_command(&service, &generation, "1", &focus_cmd(&id_b));
    let setup = inner_report_closed_focused(&[window_json(&id_a), window_json(&id_b)], &[], &id_a);
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &setup))
        .expect("setup focus executes");
    assert_eq!(
        take_outcome(&service, &generation, "1")["outcome"],
        "ready",
        "setup focus settles"
    );

    // Mixed batch: one explicit max-width float plus one implicit float
    // resolving the retained focus while staged native focus stays stale.
    file_command(&service, &generation, "2", &explicit_float(&id_a));
    file_command(&service, &generation, "3", IMPLICIT_FLOAT);
    let steady = inner_report_closed_focused(&[window_json(&id_a), window_json(&id_b)], &[], &id_a);
    let encoded = service
        .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &steady))
        .expect("batch capture drains");
    assert_eq!(
        take_outcome(&service, &generation, "2")["outcome"],
        "ready",
        "explicit float settles"
    );
    assert_eq!(
        take_outcome(&service, &generation, "3")["outcome"],
        "ready",
        "implicit float resolves retained focus and settles"
    );

    // Real canonical bytes: the capture lane binds broker keys (`source`,
    // `nativeInvalid`, `clock`, `gestureAssoc`, `gestureSettledAssoc`) onto the
    // canonical reply and nulls the requester-only `result`, so strip exactly
    // those five broker keys and decode the remainder through the public strict
    // codec. The surviving typed rows (including `intentDesired`) are the
    // real service-produced values; the section is then measured through
    // the public canonical encoder (no substring parsing). The `result`
    // lane stays outside the measured section, so its capture-lane
    // nulling cannot move the proof.
    let mut bound: serde_json::Value =
        serde_json::from_str(&encoded).expect("service reply is JSON");
    {
        let view = bound.as_object_mut().expect("service reply is an object");
        for key in [
            "source",
            "nativeInvalid",
            "clock",
            "gestureAssoc",
            "gestureSettledAssoc",
        ] {
            assert!(
                view.remove(key).is_some(),
                "capture lane binds {key}: {encoded}"
            );
        }
    }
    let decoded = decode_reply_v4(&bound.to_string()).expect("service reply decodes");
    let mut wins: Vec<&str> = decoded
        .reply
        .intent_desired
        .iter()
        .map(|write| write.win.0.as_str())
        .collect();
    wins.sort_unstable();
    assert_eq!(
        wins,
        vec![id_a.as_str(), id_b.as_str()],
        "explicit plus resolved implicit saves publish: {:?}",
        decoded.reply.intent_desired
    );
    let full = encode_reply_v4(&decoded).expect("reply re-encodes");
    assert_eq!(
        decode_reply_v4(&full).expect("canonical re-decodes"),
        decoded,
        "canonical round-trip is stable over the service rows"
    );
    let mut emptied = decoded.clone();
    emptied.reply.intent_desired.clear();
    let blank = encode_reply_v4(&emptied).expect("empty-writes reply encodes");
    let rows_bytes = full.len().saturating_sub(blank.len());
    let queued = vec![
        (
            2u64,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: tiler_core::directional::WindowSubject::Window(
                    tiler_core::directional::WindowId(id_a.clone()),
                ),
                float_rect: None,
            }),
        ),
        (
            3u64,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: tiler_core::directional::WindowSubject::CurrentFocus,
                float_rect: None,
            }),
        ),
    ];
    let bound = max_intent_save_batch_bound_v4(&queued);
    assert_eq!(
        bound,
        2 * MAX_INTENT_DESIRED_ROW_BYTES,
        "one max-width explicit plus one implicit row"
    );
    assert!(
        rows_bytes <= bound,
        "exact canonical rows fit the batch reserve: rows={rows_bytes} bound={bound}"
    );
    assert!(
        rows_bytes.saturating_add(18) <= bound.saturating_add(18),
        "exact canonical section fits the reserve plus envelope: section={} bound+18={}",
        rows_bytes.saturating_add(18),
        bound.saturating_add(18)
    );

    // Synthetic max-width leg: 128 distinct max-width rows through the same
    // public codec prove the per-row bound without a 128-window execution.
    let mut wide = decoded.clone();
    wide.reply.intent_desired = (0..128u32)
        .map(|n| IntentDesire {
            win: WindowId(format!("w{n:0127}")),
            data: Some(IntentData::new(Some(true), Some(false))),
        })
        .collect();
    let wide_full = encode_reply_v4(&wide).expect("128 max-width rows encode");
    let wide_rows = wide_full.len().saturating_sub(blank.len());
    assert!(
        wide_rows <= 128 * MAX_INTENT_DESIRED_ROW_BYTES,
        "128 canonical max-width rows fit the max-row reserve: rows={wide_rows}"
    );
}

fn implicit_resize_dir_cmd(direction: &str, mode: &str) -> String {
    format!(
        "{{\"op\":\"resize\",\"win\":{{\"currentFocus\":true}},\
          \"direction\":\"{direction}\",\"mode\":\"{mode}\",\"press\":0}}"
    )
}

fn published_rect(reply: &serde_json::Value, window: &str) -> Option<[i64; 4]> {
    let rows = reply["windows"].as_array()?;
    let row = rows.iter().find(|row| row["win"] == window)?;
    let rect = if !row["alloc"].is_null() {
        &row["alloc"]
    } else if !row["frame"].is_null() {
        &row["frame"]
    } else {
        return None;
    };
    let cells = rect.as_array()?;
    if cells.len() < 4 {
        return None;
    }
    Some([
        cells[0].as_i64()?,
        cells[1].as_i64()?,
        cells[2].as_i64()?,
        cells[3].as_i64()?,
    ])
}

fn window_json_rect(id: &str, rect: [i64; 4]) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(&window_json_at(id, 0)).expect("fixture parses");
    value["rect"] = serde_json::json!([rect[0], rect[1], rect[2], rect[3]]);
    value.to_string()
}

fn issued_focus_target(reply: &serde_json::Value) -> Option<String> {
    // Core-issued focus authority: capture replies carry the retained
    // focusRequest; presentation.foreground echoes staged native ingress
    // instead.
    reply["focusRequest"]["target"].as_str().map(str::to_owned)
}

fn observe_from_reply(
    reply: &serde_json::Value,
    prev_a: [i64; 4],
    prev_b: [i64; 4],
    prev_focused: &str,
) -> (String, [i64; 4], [i64; 4], String) {
    let rect_a = published_rect(reply, "win-a").unwrap_or(prev_a);
    let rect_b = published_rect(reply, "win-b").unwrap_or(prev_b);
    // OFFLINE settlement: the next native report carries the last
    // core-issued focus target, falling back to the previous native focus.
    let focused = issued_focus_target(reply).unwrap_or_else(|| prev_focused.to_owned());
    let native = inner_report_closed_focused(
        &[
            window_json_rect("win-a", rect_a),
            window_json_rect("win-b", rect_b),
        ],
        &[],
        &focused,
    );
    (native, rect_a, rect_b, focused)
}

fn outcome_pair(take: &serde_json::Value) -> (String, String) {
    let outcome = take["outcome"].as_str().unwrap_or("?").to_owned();
    let kind = take["kind"].as_str().unwrap_or("").to_owned();
    (outcome, kind)
}

#[allow(clippy::type_complexity)]
fn stabilize(
    service: &WorldService,
    generation: &str,
) -> (String, String, String, [i64; 4], [i64; 4], String, u64) {
    let admit = inner_report_closed_focused(
        &[window_json_at("win-a", 0), window_json_at("win-b", 500)],
        &[],
        "win-a",
    );
    let admit_reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, generation, 1, 1, &admit))
            .expect("admission capture accepted"),
    );
    // Map synthetic identity roles from the ACTUAL admitted publication:
    // left/right by published x (`alloc` falling back to `frame`).
    let xa = published_rect(&admit_reply, "win-a").expect("admission publishes win-a")[0];
    let xb = published_rect(&admit_reply, "win-b").expect("admission publishes win-b")[0];
    assert_ne!(
        xa, xb,
        "topology prerequisite: distinct published x origins"
    );
    let (left, right) = if xa < xb {
        ("win-a".to_owned(), "win-b".to_owned())
    } else {
        ("win-b".to_owned(), "win-a".to_owned())
    };
    assert_admitted_left_right(&admit_reply, &left, &right);
    let mut seq = 1u64;
    let (mut native, mut rect_a, mut rect_b, mut focused) =
        observe_from_reply(&admit_reply, [0, 0, 400, 300], [500, 0, 400, 300], "win-a");
    // Left-focus the core when the admission focus sits elsewhere, then let
    // the settled publication back into native before the sequence.
    if focused != left {
        file_command(service, generation, "1", &focus_cmd(&left));
        seq += 1;
        let setup_reply = parse(
            &service
                .evaluate_capture(&capture(ADAPTER, generation, seq, seq, &native))
                .expect("setup focus capture accepted"),
        );
        assert_eq!(
            take_outcome(service, generation, "1")["outcome"],
            "ready",
            "setup focus settles"
        );
        (native, rect_a, rect_b, focused) =
            observe_from_reply(&setup_reply, rect_a, rect_b, &focused);
    }
    seq += 1;
    let stable_reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, generation, seq, seq, &native))
            .expect("catch-up capture accepted"),
    );
    (native, rect_a, rect_b, focused) = observe_from_reply(&stable_reply, rect_a, rect_b, &focused);
    assert_eq!(
        focused, left,
        "stabilized native and core focus on the published left window"
    );
    (left, right, native, rect_a, rect_b, focused, seq + 1)
}

#[test]
fn rapid_input_live_sixty_sequence_batch_matches_serial() {
    // Bounded OFFLINE discriminator for the live 60-command burst: ten cycles of
    // focus-right, focus-left, resize-outwards-right, resize-inwards-right,
    // move-right(group-with-neighbor), move-left(group-with-neighbor), all
    // `win:{"currentFocus":true}` with resize `press:0`, driven only through the
    // genuine broker/core lane (queue parks, capture drains, take settles; no
    // test-side reimplementation of core action semantics).
    // Both legs stabilize to the same left-focused native state first, with roles
    // mapped from the ACTUAL admitted publication (live-like A-left/B-right with
    // the left window focused). The batch leg then parks all 60 and drains in ONE
    // capture over that stabilized report. The serial leg files one command per
    // capture and rebuilds the next native report from each genuine publication
    // (published frames as observed rects, the last core-issued focus effect
    // as observed focus): an OFFLINE simulation of successful actuator settlement between
    // slow presses, matching the contract's fresh-native-focus incorporation at
    // each capture. Equal logical commands, subjects, press, geometry, topology,
    // capabilities and settings. Per-command semantic outcomes plus the final
    // logical world (foreground and frames only, excluding wire counters,
    // revisions and operation tokens) must match; correct refusals are allowed in
    // both legs.
    // Resize press is 0 for every resize: out/in alternate on the same
    // window+direction key and any membership/focus change resets the shared
    // key-repeat counter, so each press is index 0 per
    // `kwin/src/world-shortcuts.ts:createResizePressCounter`.
    fn sequence() -> Vec<String> {
        let mut out = Vec::with_capacity(60);
        for _ in 0..10 {
            out.push(implicit_focus_dir_cmd("right"));
            out.push(implicit_focus_dir_cmd("left"));
            out.push(implicit_resize_dir_cmd("right", "outwards"));
            out.push(implicit_resize_dir_cmd("right", "inwards"));
            out.push(implicit_move_dir_cmd("right"));
            out.push(implicit_move_dir_cmd("left"));
        }
        out
    }
    fn logical_world(
        focused: &str,
        reply: &serde_json::Value,
        what: &str,
    ) -> (String, [i64; 4], [i64; 4]) {
        (
            focused.to_owned(),
            published_rect(reply, "win-a").unwrap_or_else(|| panic!("{what} publishes win-a")),
            published_rect(reply, "win-b").unwrap_or_else(|| panic!("{what} publishes win-b")),
        )
    }
    let cmds = sequence();
    assert_eq!(cmds.len(), 60, "ten cycles of six commands");

    // Batch leg: stabilize, park all 60 with no capture between filings, drain
    // in ONE capture over the stabilized native report.
    let (service, generation) = discovered_service();
    let (left, _right, stable_native, _rect_a, _rect_b, stable_focused, seq) =
        stabilize(&service, &generation);
    assert!(
        stable_focused == left,
        "stabilized start is left-focused: left={left} focused={stable_focused}"
    );
    for (index, cmd) in cmds.iter().enumerate() {
        file_command(&service, &generation, &(index + 2).to_string(), cmd);
    }
    for n in 2..=61u64 {
        assert_eq!(
            take_outcome(&service, &generation, &n.to_string())["outcome"],
            "pending",
            "batch command {n} parks until the single capture"
        );
    }
    let batch_reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, seq, seq, &stable_native))
            .expect("batch capture drains all 60"),
    );
    let mut batch: Vec<(String, String)> = Vec::with_capacity(60);
    for n in 2..=61u64 {
        let take = take_outcome(&service, &generation, &n.to_string());
        assert!(
            take["outcome"] == "ready" || take["outcome"] == "failed",
            "batch command {n} settles terminally: {take}"
        );
        batch.push(outcome_pair(&take));
    }

    // Serial leg: fresh service with identical stabilization, then one command
    // per capture; each capture's native report is rebuilt from the previous
    // genuine publication (settled frames and the last issued focus target).
    let (serial, serial_gen) = discovered_service();
    let (serial_left, _serial_right, mut native, mut rect_a, mut rect_b, mut focused, mut next) =
        stabilize(&serial, &serial_gen);
    assert_eq!(
        serial_left, left,
        "both legs stabilize to the same left role"
    );
    assert_eq!(
        native, stable_native,
        "both legs start from the same stabilized native report"
    );
    let mut serial_outcomes: Vec<(String, String)> = Vec::with_capacity(60);
    let mut serial_reply = serde_json::Value::Null;
    for (index, cmd) in cmds.iter().enumerate() {
        let id = (index + 2).to_string();
        file_command(&serial, &serial_gen, &id, cmd);
        let reply = parse(
            &serial
                .evaluate_capture(&capture(ADAPTER, &serial_gen, next, next, &native))
                .unwrap_or_else(|_| panic!("serial capture drains command {}", index + 1)),
        );
        next += 1;
        let take = take_outcome(&serial, &serial_gen, &id);
        assert!(
            take["outcome"] == "ready" || take["outcome"] == "failed",
            "serial command {} settles terminally: {take}",
            index + 1
        );
        serial_outcomes.push(outcome_pair(&take));
        (native, rect_a, rect_b, focused) = observe_from_reply(&reply, rect_a, rect_b, &focused);
        serial_reply = reply;
    }
    let batch_ready = batch
        .iter()
        .filter(|(outcome, _)| outcome == "ready")
        .count();
    let serial_ready = serial_outcomes
        .iter()
        .filter(|(outcome, _)| outcome == "ready")
        .count();
    assert_eq!(
        batch_ready, serial_ready,
        "settled legs agree on the ready count: batch={batch:?} serial={serial_outcomes:?}"
    );
    assert_eq!(
        batch_ready, 60,
        "settled single-batch and settled serial execute every implicit command \
         (in-batch post-prior focus resolution plus settlement-consistent native \
         evidence); batch={batch:?} serial={serial_outcomes:?}"
    );
    for (index, (outcome, kind)) in batch.iter().enumerate() {
        assert_eq!(
            (outcome.as_str(), kind.as_str()),
            ("ready", ""),
            "settled batch command {} carries no reject kind: {batch:?}",
            index + 1
        );
    }
    assert_eq!(
        batch, serial_outcomes,
        "batch-vs-serial discriminator: same 60 implicit commands from the same \
         stabilized left-focused state with settlement-consistent native evidence \
         must settle identically per command"
    );
    assert_eq!(
        logical_world(
            &issued_focus_target(&batch_reply).unwrap_or_else(|| stable_focused.clone()),
            &batch_reply,
            "batch"
        ),
        logical_world(&focused, &serial_reply, "serial"),
        "final logical world (foreground and frames) matches between batch and serial"
    );
}

#[test]
fn rapid_input_live_split_batch_unchanged_focus_matches_one_batch() {
    // Capture-boundary companion to the settled 60-command replay: the same
    // stabilized left-focused state and the first nine implicit commands of
    // the 60-sequence, drained either in ONE capture or SPLIT 7+1+1 across
    // three captures while staged native focus stays UNCHANGED (lagging).
    // Both legs must settle every command ready with identical per-command
    // outcomes: the execution-time subject is core state for every queued
    // command, never the lagging capture. A following CHANGED (caught-up)
    // native control succeeds as well.
    let (service, generation) = discovered_service();
    let (left, _right, stable_native, _rect_a, _rect_b, stable_focused, seq) =
        stabilize(&service, &generation);
    assert!(
        stable_focused == left,
        "stabilized start is left-focused: left={left} focused={stable_focused}"
    );
    // First nine commands of the 60-sequence: one full cycle plus focusR,
    // focusL, resizeR-out.
    let cmds = vec![
        implicit_focus_dir_cmd("right"),
        implicit_focus_dir_cmd("left"),
        implicit_resize_dir_cmd("right", "outwards"),
        implicit_resize_dir_cmd("right", "inwards"),
        implicit_move_dir_cmd("right"),
        implicit_move_dir_cmd("left"),
        implicit_focus_dir_cmd("right"),
        implicit_focus_dir_cmd("left"),
        implicit_resize_dir_cmd("right", "outwards"),
    ];
    // One-batch leg: all nine park, one capture over the stabilized
    // unchanged native drains them through in-batch post-prior focus.
    for (index, cmd) in cmds.iter().enumerate() {
        file_command(&service, &generation, &(index + 2).to_string(), cmd);
    }
    service
        .evaluate_capture(&capture(ADAPTER, &generation, seq, seq, &stable_native))
        .expect("nine-command batch drains");
    let mut one: Vec<(String, String)> = Vec::with_capacity(9);
    for n in 2..=10u64 {
        let take = take_outcome(&service, &generation, &n.to_string());
        assert_eq!(
            take["outcome"], "ready",
            "one-batch command {n} settles: {take}"
        );
        one.push(outcome_pair(&take));
    }
    // Split leg: fresh service, same stabilization, same commands split 7+1+1
    // across three captures carrying the SAME unchanged native report.
    let (split, split_gen) = discovered_service();
    let (split_left, _split_right, split_native, _sra, _srb, mut focused, mut next) =
        stabilize(&split, &split_gen);
    assert_eq!(
        split_left, left,
        "both legs stabilize to the same left role"
    );
    assert_eq!(
        split_native, stable_native,
        "both legs start from the same stabilized native report"
    );
    let mut split_outcomes: Vec<(String, String)> = Vec::with_capacity(9);
    for (index, cmd) in cmds[0..7].iter().enumerate() {
        file_command(&split, &split_gen, &(index + 2).to_string(), cmd);
    }
    let leg1 = parse(
        &split
            .evaluate_capture(&capture(ADAPTER, &split_gen, next, next, &split_native))
            .expect("split seven-command batch drains"),
    );
    next += 1;
    for n in 2..=8u64 {
        let take = take_outcome(&split, &split_gen, &n.to_string());
        assert_eq!(
            take["outcome"], "ready",
            "split head command {n} settles: {take}"
        );
        split_outcomes.push(outcome_pair(&take));
    }
    focused = issued_focus_target(&leg1).unwrap_or(focused);
    file_command(&split, &split_gen, "9", &cmds[7]);
    let leg2 = parse(
        &split
            .evaluate_capture(&capture(ADAPTER, &split_gen, next, next, &split_native))
            .expect("split single-command capture drains"),
    );
    next += 1;
    let middle = take_outcome(&split, &split_gen, "9");
    assert_eq!(
        middle["outcome"], "ready",
        "cross-capture implicit command keeps its core subject over unchanged native: {middle}"
    );
    split_outcomes.push(outcome_pair(&middle));
    focused = issued_focus_target(&leg2).unwrap_or(focused);
    file_command(&split, &split_gen, "10", &cmds[8]);
    let leg3 = parse(
        &split
            .evaluate_capture(&capture(ADAPTER, &split_gen, next, next, &split_native))
            .expect("split trailing capture drains"),
    );
    next += 1;
    let tail = take_outcome(&split, &split_gen, "10");
    assert_eq!(
        tail["outcome"], "ready",
        "trailing implicit command keeps its core subject over unchanged native: {tail}"
    );
    split_outcomes.push(outcome_pair(&tail));
    focused = issued_focus_target(&leg3).unwrap_or(focused);
    assert_eq!(
        split_outcomes, one,
        "split batches over unchanged native match the one-batch leg per command"
    );
    // CHANGED control: the same resize shape with caught-up native evidence
    // (frames plus the last core-issued focus target) succeeds as well.
    let ra = published_rect(&leg3, "win-a").expect("split publishes win-a");
    let rb = published_rect(&leg3, "win-b").expect("split publishes win-b");
    let changed_native = inner_report_closed_focused(
        &[window_json_rect("win-a", ra), window_json_rect("win-b", rb)],
        &[],
        &focused,
    );
    file_command(
        &split,
        &split_gen,
        "11",
        &implicit_resize_dir_cmd("right", "outwards"),
    );
    split
        .evaluate_capture(&capture(ADAPTER, &split_gen, next, next, &changed_native))
        .expect("changed-native capture drains");
    let fresh = take_outcome(&split, &split_gen, "11");
    assert_eq!(
        fresh["outcome"], "ready",
        "same implicit resize with caught-up native evidence succeeds: {fresh}"
    );
}

// Checked-service release workload (change record `kde-rebuild-v4` row 7 /
// decision 7): reproducible, offline-only, real `WorldService` lifecycle.
// Cold: 30 fresh worlds, each admitting 200 windows on its first capture;
// the timed region starts just before `evaluate_capture` ingress and ends
// when the checked reply bytes are returned (fixture string building and
// the oracle JSON parse run outside it). Handshake and settings are timed
// in their own vectors and disclosed separately, never folded into the
// cold capture samples. Warm: 30 samples, each on a fresh service with a
// stable admitted 200-window inventory, filing one deterministic
// 16-command FIFO burst (8 x focus a distinct window, then
// toggle-maximize `currentFocus`). Every checked Queue/Take request, the
// capture envelope and the reply vectors are prebuilt before `burst_start`,
// so the timed region runs only the service calls, from the first Queue
// ingress through the last Take reply, with the single draining capture
// timed separately inside the burst. Both request bytes (read from the
// prebuilt strings outside the timer) and reply bytes (the raw checked
// strings each call returns) are reported per lane (queue, capture, take).
// The final-state oracle (last focus target plus the per-window maximized
// `cmdSeq` map, one entry per toggle) distinguishes reorder, because a
// reordered toggle attaches to a different subject, and distinguishes
// duplicate execution, because a second toggle of the same window would
// un-maximize it. Exact capture redelivery, lost-take re-service and
// re-filing of all 16 exact requests (each must duplicate with its
// original globalCmd, including the non-idempotent toggles) are asserted
// outside the timed region and must not re-execute. The 100 ms / 50 ms
// p95 figures in decision 7 are investigation budgets only: they are
// printed for comparison and never asserted as a gate. Small-broker and
// structural timing gates live elsewhere and are untouched by this test.
// Sample counts are profile-dependent by design, not a skip: release
// (the runnable command below) keeps all 30 samples per leg for the
// p50/p95/max distribution, while a normal debug run exercises each
// full semantic workload exactly once per leg (200-window admission,
// all 16 operations and their replays) so dev iterations stay fast.
// Runnable (build first, then time alone, serially):
//   cargo test --release --locked -p omnitiler --test rapid_input_burst \
//     checked_service_release_latency_workload -- --exact --nocapture \
//     --test-threads=1
#[test]
fn checked_service_release_latency_workload() {
    use std::time::Instant;

    const WINDOWS: usize = 200;
    const COLD_SAMPLES: usize = if cfg!(debug_assertions) { 1 } else { 30 };
    const WARM_SAMPLES: usize = if cfg!(debug_assertions) { 1 } else { 30 };
    const BURST_COMMANDS: usize = 16;

    fn window_id(n: usize) -> String {
        format!("win-{n:03}")
    }

    fn windows_200() -> Vec<String> {
        (0..WINDOWS).map(|n| window_json(&window_id(n))).collect()
    }

    // Same percentile definition as `tiler-core/examples/report_scale.rs`:
    // median is the middle order statistic, p95 is the order statistic at
    // ceil(0.95 * n) (1-based), max is the largest sample.
    fn median_p95_max(mut values: Vec<f64>) -> (f64, f64, f64) {
        values.sort_by(f64::total_cmp);
        let n = values.len();
        let p95_index = ((n as f64) * 0.95).ceil() as usize - 1;
        (values[n / 2], values[p95_index.min(n - 1)], values[n - 1])
    }

    fn maximized_map(reply: &serde_json::Value) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = reply["windows"]
            .as_array()
            .expect("reply publishes windows")
            .iter()
            .filter_map(|row| {
                let entry = row["state"]
                    .as_array()?
                    .iter()
                    .find(|s| s["property"] == "maximized" && s["value"] == true)?;
                Some((
                    row["win"].as_str().expect("window id").to_owned(),
                    entry["cmdSeq"].as_str().expect("cmdSeq").to_owned(),
                ))
            })
            .collect();
        out.sort();
        out
    }

    // Exact admitted identity set, checked outside every timer: the
    // published rows must be precisely the prebuilt win-000..win-199,
    // once each (no duplicates or substitutions), in every capture.
    fn assert_admitted_200(reply: &serde_json::Value, expected: &[String], what: &str) {
        let mut ids: Vec<String> = reply["windows"]
            .as_array()
            .unwrap_or_else(|| panic!("{what} publishes windows: {reply}"))
            .iter()
            .map(|row| {
                row["win"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{what} row has no win id: {row}"))
                    .to_owned()
            })
            .collect();
        ids.sort();
        assert_eq!(
            ids.as_slice(),
            expected,
            "{what} admits exactly win-000..win-199"
        );
    }

    let expected_ids: Vec<String> = (0..WINDOWS).map(window_id).collect();
    // Cold leg: fresh world per sample, first admitted 200-window capture.
    let mut cold_ms: Vec<f64> = Vec::with_capacity(COLD_SAMPLES);
    let mut handshake_ms: Vec<f64> = Vec::with_capacity(COLD_SAMPLES);
    let mut settings_ms: Vec<f64> = Vec::with_capacity(COLD_SAMPLES);
    let mut cold_first_request_bytes = 0usize;
    let mut cold_first_reply_bytes = 0usize;
    let mut cold_max_reply_bytes = 0usize;
    for sample in 0..COLD_SAMPLES {
        let service = WorldService::new();
        let start = Instant::now();
        let discovery_raw = service
            .discover(&handshake_req_owned(ADAPTER, "req-0", false))
            .expect("handshake routes");
        handshake_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        let discovery = parse(&discovery_raw);
        assert_eq!(discovery["ready"], false, "handshake pre-ready");
        let generation = discovery["gen"].as_str().expect("gen").to_owned();
        let start = Instant::now();
        let settings_raw = service
            .discover(&settings_req(ADAPTER, &generation, "settings-0"))
            .expect("settings routes");
        settings_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(parse(&settings_raw)["status"], "accepted");
        // Fixture construction runs outside the timed region.
        let inner = inner_report_closed_focused(&windows_200(), &[], "win-000");
        let envelope = capture(ADAPTER, &generation, 1, 1, &inner);
        let start = Instant::now();
        let reply_raw = service
            .evaluate_capture(&envelope)
            .expect("cold capture accepted");
        cold_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        if sample == 0 {
            cold_first_request_bytes = envelope.len();
            cold_first_reply_bytes = reply_raw.len();
        }
        cold_max_reply_bytes = cold_max_reply_bytes.max(reply_raw.len());
        let reply = parse(&reply_raw);
        assert_eq!(reply["ready"], true, "cold sample {sample}: {reply}");
        assert_eq!(reply["rev"], "1", "cold sample {sample} first revision");
        assert_admitted_200(&reply, &expected_ids, &format!("cold sample {sample}"));
    }

    // Warm leg: stable 200-window inventory, one 16-command FIFO burst.
    let mut warm_ms: Vec<f64> = Vec::with_capacity(WARM_SAMPLES);
    let mut warm_capture_ms: Vec<f64> = Vec::with_capacity(WARM_SAMPLES);
    let mut warm_first_queue_bytes = 0usize;
    let mut warm_first_capture_bytes = 0usize;
    let mut warm_first_take_bytes = 0usize;
    let mut warm_max_queue_bytes = 0usize;
    let mut warm_max_capture_bytes = 0usize;
    let mut warm_max_take_bytes = 0usize;
    let mut warm_first_queue_req_bytes = 0usize;
    let mut warm_first_capture_req_bytes = 0usize;
    let mut warm_first_take_req_bytes = 0usize;
    let mut warm_max_queue_req_bytes = 0usize;
    let mut warm_max_capture_req_bytes = 0usize;
    let mut warm_max_take_req_bytes = 0usize;
    for sample in 0..WARM_SAMPLES {
        let (service, generation) = discovered_service();
        let inner = inner_report_closed_focused(&windows_200(), &[], "win-000");
        let admit_raw = service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("warm admission accepted");
        let admit = parse(&admit_raw);
        assert_eq!(admit["ready"], true, "warm sample {sample} admission");
        assert_admitted_200(
            &admit,
            &expected_ids,
            &format!("warm sample {sample} admission"),
        );
        // Deterministic noncommuting burst: each toggle-maximize binds to
        // the immediately preceding focus via `currentFocus`, so any
        // reorder changes which window each toggle maximizes.
        let mut commands: Vec<String> = Vec::with_capacity(BURST_COMMANDS);
        for k in 0..8 {
            commands.push(focus_cmd(&window_id(k)));
            commands
                .push("{\"op\":\"toggle-maximize\",\"win\":{\"currentFocus\":true}}".to_owned());
        }
        assert_eq!(commands.len(), BURST_COMMANDS);
        let ids: Vec<String> = (1..=BURST_COMMANDS as u64).map(|n| n.to_string()).collect();
        // Prebuild every checked request, the capture envelope and the reply
        // vectors before the timer starts, so the timed region is only the
        // service calls themselves (first request ingress through the last
        // Take reply); request byte counts are read from these prebuilt
        // strings outside the timer.
        let queue_reqs: Vec<String> = ids
            .iter()
            .zip(&commands)
            .map(|(id, command)| queue_req(&generation, "panel-1", id, command))
            .collect();
        let take_reqs: Vec<String> = ids
            .iter()
            .map(|id| take_req(&generation, "panel-1", id))
            .collect();
        let burst_envelope = capture(ADAPTER, &generation, 2, 2, &inner);
        let queue_req_bytes: usize = queue_reqs.iter().map(String::len).sum();
        let capture_req_bytes = burst_envelope.len();
        let take_req_bytes: usize = take_reqs.iter().map(String::len).sum();
        let mut queue_raws: Vec<String> = Vec::with_capacity(BURST_COMMANDS);
        let mut take_raws: Vec<String> = Vec::with_capacity(BURST_COMMANDS);
        // Timed burst: first Queue send through last checked Take reply.
        let burst_start = Instant::now();
        for request in &queue_reqs {
            queue_raws.push(
                service
                    .queue_command(SENDER_A, request)
                    .expect("queue in-band"),
            );
        }
        let capture_start = Instant::now();
        let capture_raw = service
            .evaluate_capture(&burst_envelope)
            .expect("burst capture drains");
        warm_capture_ms.push(capture_start.elapsed().as_secs_f64() * 1000.0);
        for request in &take_reqs {
            take_raws.push(
                service
                    .take_result(SENDER_A, request)
                    .expect("take in-band"),
            );
        }
        warm_ms.push(burst_start.elapsed().as_secs_f64() * 1000.0);
        let queue_bytes: usize = queue_raws.iter().map(String::len).sum();
        let take_bytes: usize = take_raws.iter().map(String::len).sum();
        if sample == 0 {
            warm_first_queue_bytes = queue_bytes;
            warm_first_capture_bytes = capture_raw.len();
            warm_first_take_bytes = take_bytes;
            warm_first_queue_req_bytes = queue_req_bytes;
            warm_first_capture_req_bytes = capture_req_bytes;
            warm_first_take_req_bytes = take_req_bytes;
        }
        warm_max_queue_bytes = warm_max_queue_bytes.max(queue_bytes);
        warm_max_capture_bytes = warm_max_capture_bytes.max(capture_raw.len());
        warm_max_take_bytes = warm_max_take_bytes.max(take_bytes);
        warm_max_queue_req_bytes = warm_max_queue_req_bytes.max(queue_req_bytes);
        warm_max_capture_req_bytes = warm_max_capture_req_bytes.max(capture_req_bytes);
        warm_max_take_req_bytes = warm_max_take_req_bytes.max(take_req_bytes);
        // Oracle runs outside the timed region, on the returned raw bytes.
        let queues: Vec<serde_json::Value> = queue_raws.iter().map(|raw| parse(raw)).collect();
        for (index, queue) in queues.iter().enumerate() {
            assert_eq!(
                queue["outcome"],
                "queued",
                "warm sample {sample} queue {} must not refuse: {queue}",
                index + 1
            );
        }
        let queue_cmds: Vec<String> = queues
            .iter()
            .map(|queue| {
                queue["globalCmd"]
                    .as_str()
                    .expect("queue globalCmd")
                    .to_owned()
            })
            .collect();
        for pair in queue_cmds.windows(2) {
            let (prev, next): (u64, u64) = (
                pair[0].parse().expect("globalCmd parses"),
                pair[1].parse().expect("globalCmd parses"),
            );
            assert!(
                prev < next,
                "queue globalCmd strictly increasing: {queue_cmds:?}"
            );
        }
        let burst_reply = parse(&capture_raw);
        assert_eq!(
            burst_reply["ready"], true,
            "warm sample {sample} burst capture: {burst_reply}"
        );
        assert_eq!(burst_reply["rev"], "2", "burst drains in one revision");
        assert_admitted_200(
            &burst_reply,
            &expected_ids,
            &format!("warm sample {sample} burst"),
        );
        assert_eq!(
            burst_reply["focusRequest"]["target"], "win-007",
            "final focus is the last focused window in FIFO order"
        );
        let takes: Vec<serde_json::Value> = take_raws.iter().map(|raw| parse(raw)).collect();
        for (index, take) in takes.iter().enumerate() {
            assert_eq!(
                take["outcome"],
                "ready",
                "warm sample {sample} take {} settles terminally: {take}",
                index + 1
            );
            assert_eq!(
                take["globalCmd"].as_str(),
                Some(queue_cmds[index].as_str()),
                "terminal result {} carries its filed globalCmd",
                index + 1
            );
        }
        let expected_maximized: Vec<(String, String)> = (0..8)
            .map(|k| (window_id(k), queue_cmds[2 * k + 1].clone()))
            .collect();
        assert_eq!(
            maximized_map(&burst_reply),
            expected_maximized,
            "warm sample {sample}: each toggle maximized its preceding focus, once"
        );
        // Exact replay without duplicate effects, outside the timed region.
        let replay_raw = service
            .evaluate_capture(&burst_envelope)
            .expect("capture replay");
        assert_eq!(
            replay_raw, capture_raw,
            "warm sample {sample} capture redelivery replays byte-identically"
        );
        for (request, first) in take_reqs.iter().zip(&take_raws) {
            let again = service
                .take_result(SENDER_A, request)
                .expect("take replay in-band");
            assert_eq!(again, *first, "lost take reply re-serves verbatim");
        }
        // Refile every exact original request: each duplicates with its
        // original globalCmd and must not execute. The toggle-maximizes are
        // non-idempotent (a second execution would un-maximize), so the
        // unchanged maximized map on the follow-up capture below proves
        // the whole burst ran exactly once.
        for (index, request) in queue_reqs.iter().enumerate() {
            let duplicate = parse(
                &service
                    .queue_command(SENDER_A, request)
                    .expect("duplicate queue in-band"),
            );
            assert_eq!(
                duplicate["outcome"],
                "duplicate",
                "re-filing request {} duplicates",
                index + 1
            );
            assert_eq!(
                duplicate["globalCmd"].as_str(),
                Some(queue_cmds[index].as_str()),
                "duplicate {} carries the original globalCmd",
                index + 1
            );
        }
        let follow_up = parse(
            &service
                .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
                .expect("follow-up capture accepted"),
        );
        assert_eq!(
            follow_up["rev"], "3",
            "follow-up advances exactly one revision"
        );
        assert_admitted_200(
            &follow_up,
            &expected_ids,
            &format!("warm sample {sample} follow-up"),
        );
        assert_eq!(
            maximized_map(&follow_up),
            expected_maximized,
            "warm sample {sample}: replay and duplicate queue did not re-execute"
        );
    }

    let cold_first_ms = cold_ms[0];
    let warm_first_ms = warm_ms[0];
    let warm_capture_first_ms = warm_capture_ms[0];
    let (cold_p50, cold_p95, cold_max) = median_p95_max(cold_ms);
    let (handshake_p50, handshake_p95, handshake_max) = median_p95_max(handshake_ms);
    let (settings_p50, settings_p95, settings_max) = median_p95_max(settings_ms);
    let (warm_p50, warm_p95, warm_max) = median_p95_max(warm_ms);
    let (capture_p50, capture_p95, capture_max) = median_p95_max(warm_capture_ms);
    eprintln!(
        "checked-service release workload: windows={WINDOWS} cold_samples={COLD_SAMPLES} \
         warm_samples={WARM_SAMPLES} burst_commands={BURST_COMMANDS}"
    );
    eprintln!(
        "cold first-capture ms: first={:.3} p50={cold_p50:.3} p95={cold_p95:.3} max={cold_max:.3} \
         request_bytes(first)={cold_first_request_bytes} reply_bytes(first)={cold_first_reply_bytes} \
         reply_bytes(max)={cold_max_reply_bytes}",
        cold_first_ms
    );
    eprintln!(
        "handshake ms (disclosed, excluded from cold): p50={handshake_p50:.3} \
         p95={handshake_p95:.3} max={handshake_max:.3}"
    );
    eprintln!(
        "settings ms (disclosed, excluded from cold): p50={settings_p50:.3} \
         p95={settings_p95:.3} max={settings_max:.3}"
    );
    eprintln!(
        "warm burst ms (first Queue through last Take): first={:.3} p50={warm_p50:.3} \
         p95={warm_p95:.3} max={warm_max:.3}",
        warm_first_ms
    );
    eprintln!(
        "warm capture-in-burst ms: first={:.3} p50={capture_p50:.3} p95={capture_p95:.3} \
         max={capture_max:.3}",
        warm_capture_first_ms
    );
    eprintln!(
        "warm bytes per sample, request/reply per lane (16 queue / capture / 16 take): \
         first=({warm_first_queue_req_bytes}/{warm_first_queue_bytes} \
         {warm_first_capture_req_bytes}/{warm_first_capture_bytes} \
         {warm_first_take_req_bytes}/{warm_first_take_bytes}) \
         max=({warm_max_queue_req_bytes}/{warm_max_queue_bytes} \
         {warm_max_capture_req_bytes}/{warm_max_capture_bytes} \
         {warm_max_take_req_bytes}/{warm_max_take_bytes})"
    );
    eprintln!(
        "investigation budgets (not gates, decision 7): cold p95 vs 100 ms, warm p95 vs 50 ms"
    );
}

// Offline, observational service sequence: the fixture supplies the cuts and
// stages the corresponding reports. It does not exercise or prove producer
// continuity, native capture cost, or physical key-to-publication latency.
// cargo test --release --locked -p omnitiler --test rapid_input_burst \
//   checked_service_sealed_float_bursts -- --exact --nocapture --test-threads=1
#[test]
fn checked_service_sealed_float_bursts() {
    use std::time::Instant;

    const SAMPLES: usize = if cfg!(debug_assertions) { 1 } else { 30 };
    const TOGGLE: &str = r#"{"op":"toggle-float","win":{"currentFocus":true},"rect":null}"#;
    const SENDER_B: &str = ":1.43";
    const SINGLE_CUT: &[usize] = &[9];
    const ALTERNATING: &[usize] = &[1, 2, 3, 2, 1];

    struct Filing {
        sender: &'static str,
        queue: String,
        take: String,
    }

    fn assert_membership(
        raw: &serde_json::Value,
        ids: &[String],
        expected_float: [bool; 2],
        rev: u64,
    ) {
        assert_eq!(raw["ready"], true, "capture must incorporate: {raw}");
        assert_eq!(raw["rev"], rev.to_string(), "one revision per report cut");
        let mut rows: Vec<_> = raw["windows"]
            .as_array()
            .expect("published windows")
            .iter()
            .map(|row| {
                (
                    row["win"].as_str().expect("window identity"),
                    row["floating"].as_bool().expect("float membership"),
                )
            })
            .collect();
        rows.sort_unstable();
        assert_eq!(rows.len(), ids.len());
        for (index, ((win, floating), expected_id)) in rows.into_iter().zip(ids).enumerate() {
            assert_eq!(win, expected_id.as_str(), "admitted roster at rev {rev}");
            assert_eq!(
                floating,
                expected_float.get(index).copied().unwrap_or(false),
                "{win} float membership at rev {rev}"
            );
        }
    }

    for window_count in [50, 200] {
        let ids: Vec<_> = (0..window_count).map(|n| format!("win-{n:03}")).collect();
        let windows: Vec<_> = ids.iter().map(|id| window_json(id)).collect();
        let unchanged = inner_report_closed_focused(&windows, &[], &ids[0]);
        let alternate = inner_report_closed_focused(&windows, &[], &ids[1]);
        // Identical Queue/Take load in every leg: eight local toggles and one
        // external toggle between the fourth and fifth local filings.
        for (label, sealed, sizes) in [
            ("one-revision unchanged control", false, SINGLE_CUT),
            ("sealed unchanged", true, SINGLE_CUT),
            ("sealed alternating", true, ALTERNATING),
        ] {
            let mut publication_ms = Vec::with_capacity(SAMPLES);
            let mut burst_ms = Vec::with_capacity(SAMPLES);
            let mut max_bytes = [0_usize; 6];
            for _ in 0..SAMPLES {
                let (service, generation) = discovered_service();
                let admit = service
                    .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &unchanged))
                    .expect("admission reply");
                assert_membership(&parse(&admit), &ids, [false; 2], 1);

                let filings: Vec<_> = (1..=9)
                    .map(|n| {
                        let (sender, client) = if n == 5 {
                            (SENDER_B, "panel-1")
                        } else {
                            (SENDER_A, "shortcut-1")
                        };
                        let id = n.to_string();
                        Filing {
                            sender,
                            queue: queue_req(&generation, client, &id, TOGGLE),
                            take: take_req(&generation, client, &id),
                        }
                    })
                    .collect();
                let envelopes: Vec<_> = sizes
                    .iter()
                    .enumerate()
                    .map(|(cut, _)| {
                        let after = cut as u64 + 1;
                        let focused = &ids[cut % 2];
                        let inner = if cut % 2 == 0 { &unchanged } else { &alternate };
                        let report = if sealed {
                            let events = if cut == 0 {
                                serde_json::json!([])
                            } else {
                                serde_json::json!([{"kind": "foreground", "win": focused}])
                            };
                            let interval = serde_json::json!({
                                "after": after.to_string(),
                                "events": events,
                            });
                            inner.replace(
                                "\"focusChanges\":null",
                                &format!("\"focusChanges\":{interval}"),
                            )
                        } else {
                            inner.clone()
                        };
                        capture(ADAPTER, &generation, after + 1, after + 1, &report)
                    })
                    .collect();

                let mut queues = Vec::with_capacity(filings.len());
                let mut takes = Vec::with_capacity(filings.len());
                let mut captures = Vec::with_capacity(sizes.len());
                let mut through_publication_ms = 0.0;
                let mut offset = 0;
                let start = Instant::now();
                for (size, envelope) in sizes.iter().zip(&envelopes) {
                    let segment = &filings[offset..offset + size];
                    for filing in segment {
                        queues.push(
                            service
                                .queue_command(filing.sender, &filing.queue)
                                .expect("Queue reply"),
                        );
                    }
                    captures.push(service.evaluate_capture(envelope).expect("capture reply"));
                    through_publication_ms = start.elapsed().as_secs_f64() * 1000.0;
                    for filing in segment {
                        takes.push(
                            service
                                .take_result(filing.sender, &filing.take)
                                .expect("Take reply"),
                        );
                    }
                    offset += size;
                }
                burst_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                publication_ms.push(through_publication_ms);

                let bytes = [
                    filings.iter().map(|f| f.queue.len()).sum(),
                    queues.iter().map(String::len).sum(),
                    envelopes.iter().map(String::len).sum(),
                    captures.iter().map(String::len).sum(),
                    filings.iter().map(|f| f.take.len()).sum(),
                    takes.iter().map(String::len).sum(),
                ];
                for (max, actual) in max_bytes.iter_mut().zip(bytes) {
                    *max = (*max).max(actual);
                }

                let mut expected_float = [false; 2];
                let mut previous_global = 0;
                let mut offset = 0;
                for (cut, (&size, raw)) in sizes.iter().zip(&captures).enumerate() {
                    let target = cut % 2;
                    for index in offset..offset + size {
                        let queue = parse(&queues[index]);
                        let take = parse(&takes[index]);
                        assert_eq!(queue["outcome"], "queued", "{queue}");
                        let global: u64 = queue["globalCmd"]
                            .as_str()
                            .expect("global FIFO token")
                            .parse()
                            .expect("numeric FIFO token");
                        assert!(global > previous_global, "FIFO across both senders");
                        previous_global = global;
                        assert_eq!(take["outcome"], "ready", "{take}");
                        assert_eq!(take["globalCmd"], queue["globalCmd"]);
                        assert_eq!(take["result"]["op"], "toggle-float");
                        assert_eq!(take["result"]["win"], ids[target].as_str());
                        expected_float[target] = !expected_float[target];
                    }
                    assert_membership(&parse(raw), &ids, expected_float, cut as u64 + 2);
                    offset += size;
                }
                assert_eq!(offset, filings.len());
            }

            // Same order statistics as the existing checked-service workload.
            publication_ms.sort_by(f64::total_cmp);
            burst_ms.sort_by(f64::total_cmp);
            let p50 = SAMPLES / 2;
            let p95 = (SAMPLES * 95).div_ceil(100) - 1;
            let [
                queue_req,
                queue_reply,
                capture_req,
                capture_reply,
                take_req,
                take_reply,
            ] = max_bytes;
            eprintln!(
                "checked-service float bursts: windows={window_count} case={label} \
                 cuts={} samples={SAMPLES}\n\
                 first Queue to last publication ms: p50={:.3} p95={:.3}; \
                 through last Take ms: p50={:.3} p95={:.3}\n\
                 max sample bytes, request/reply: Queue={queue_req}/{queue_reply} \
                 capture={capture_req}/{capture_reply} Take={take_req}/{take_reply}",
                sizes.len(),
                publication_ms[p50],
                publication_ms[p95],
                burst_ms[p50],
                burst_ms[p95],
            );
        }
    }
}
