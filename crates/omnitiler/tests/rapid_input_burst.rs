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
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"lastWrite\":null}}"
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
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"lastWrite\":null}}"
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
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[{}],\"native\":[],\
          \"focused\":\"{focused}\",\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{},\
          \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
          \"rendererInput\":null}}",
        windows.join(","),
        closed
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(","),
        disabled_presentation()
    )
}

/// v4 inner report carrying valid host-owned native rows: one ordered
/// native desktop observed on DP-1. The bound handshake advertises
/// `hostOwned:true`, so this evidence is admission-valid.
fn inner_report_host_native(windows: &[String], focused: &str) -> String {
    format!(
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[],\
          \"native\":[{{\"out\":\"DP-1\",\"order\":[\"n-0\"],\
          \"current\":\"n-0\",\"occupancy\":[{{\"native\":\"n-0\",\"count\":1}}]}}],\
          \"focused\":\"{focused}\",\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{},\
          \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
          \"rendererInput\":null}}",
        windows.join(","),
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
          \"capabilities\":{{\"hostOwned\":{host_owned},\"equalCount\":false,\"birthClear\":false,\
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
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"lastWrite\":null}}"
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
    // canonical codec, and the exact `intentWrites` section - full encode
    // minus the empty-writes encode plus the 18-byte empty section - fits
    // the pre-mutation batch reserve. A second leg mutates the decoded
    // reply to 128 synthetic max-width rows and re-encodes through the
    // same public codec to prove the 128-row max-width bound without a
    // 128-window execution.
    use tiler_core::boundary::CoreCommand;
    use tiler_core::directional::WindowId;
    use tiler_core::intent_data::{IntentData, encode_intent_data};
    use tiler_core::world_report::{IntentWrite, WorldCommand};
    use tiler_core::world_sizing::MAX_INTENT_SAVE_ROW_BYTES;
    use tiler_core::world_sizing_v4::max_intent_save_batch_bound_v4;
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
    // `nativeInvalid`, `clock`, `gestureAssoc`) onto the canonical reply
    // and nulls the requester-only `result`, so strip exactly those four
    // broker keys and decode the remainder through the public strict
    // codec. The surviving typed rows (including `intentWrites`) are the
    // real service-produced values; the section is then measured through
    // the public canonical encoder (no substring parsing). The `result`
    // lane stays outside the measured section, so its capture-lane
    // nulling cannot move the proof.
    let mut bound: serde_json::Value =
        serde_json::from_str(&encoded).expect("service reply is JSON");
    {
        let view = bound.as_object_mut().expect("service reply is an object");
        for key in ["source", "nativeInvalid", "clock", "gestureAssoc"] {
            assert!(
                view.remove(key).is_some(),
                "capture lane binds {key}: {encoded}"
            );
        }
    }
    let decoded = decode_reply_v4(&bound.to_string()).expect("service reply decodes");
    let mut wins: Vec<&str> = decoded
        .reply
        .intent_writes
        .iter()
        .map(|write| write.win.0.as_str())
        .collect();
    wins.sort_unstable();
    assert_eq!(
        wins,
        vec![id_a.as_str(), id_b.as_str()],
        "explicit plus resolved implicit saves publish: {:?}",
        decoded.reply.intent_writes
    );
    let full = encode_reply_v4(&decoded).expect("reply re-encodes");
    assert_eq!(
        decode_reply_v4(&full).expect("canonical re-decodes"),
        decoded,
        "canonical round-trip is stable over the service rows"
    );
    let mut emptied = decoded.clone();
    emptied.reply.intent_writes.clear();
    let blank = encode_reply_v4(&emptied).expect("empty-writes reply encodes");
    let rows_bytes = full.len().saturating_sub(blank.len());
    let queued = vec![
        (
            2u64,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: id_a.clone(),
                float_rect: None,
            }),
        ),
        (
            3u64,
            WorldCommand::Implicit(Box::new(WorldCommand::Operation(
                CoreCommand::ToggleFloat {
                    window: String::new(),
                    float_rect: None,
                },
            ))),
        ),
    ];
    let bound = max_intent_save_batch_bound_v4(&queued);
    assert_eq!(
        bound,
        2 * MAX_INTENT_SAVE_ROW_BYTES,
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
    wide.reply.intent_writes = (0..128u32)
        .map(|n| IntentWrite {
            win: WindowId(format!("w{n:0127}")),
            data: Some(encode_intent_data(&IntentData::new(
                Some(true),
                Some(false),
            ))),
        })
        .collect();
    let wide_full = encode_reply_v4(&wide).expect("128 max-width rows encode");
    let wide_rows = wide_full.len().saturating_sub(blank.len());
    assert!(
        wide_rows <= 128 * MAX_INTENT_SAVE_ROW_BYTES,
        "128 canonical max-width rows fit the max-row reserve: rows={wide_rows}"
    );
}
