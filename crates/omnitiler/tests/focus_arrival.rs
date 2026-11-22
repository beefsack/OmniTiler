//! Focus-arrival regression, Unit 6 amendment (offline, checked service).
//!
//! Native journal anchor (unit6-live-2026-11-19T21-42-23-040Z, rev 24):
//! a send-follow transfer (`op-5`, `ws-17`) and its explicit focus request
//! (`focus-24`, `afterShownOp=op-5`) share one command, but the focus row
//! names the pre-move home (`ws-13`); the adapter then records
//! `decision=no-call reason=identity-stale` and the op later settles
//! `completed-with-failures focus=unproven`.
//!
//! The clean offline equivalent through the real checked broker:
//! append a native desktop and its binding, send win-a to that appended
//! workspace with `follow=true`, and assert the same reply's focus request
//! plans against the destination home with `afterShown.op` equal to the
//! transfer op, and that the command does not settle before arrival.
//! A companion perturbation control keeps a foreign activation plus a
//! workspace switch mid-flight from planning focus on the wrong window or
//! settling as complete.
//!
//! No wire or capability change. The positive case failed on the pre-fix
//! core, whose focus request froze the source home.

use omnitiler::world_service::WorldService;
use serde_json::{Value, json};

const ADAPTER: &str = "kwin-focus-arrival-1";
const UUID_A: &str = "550e8400-e29b-41d4-a716-446655440001";
const UUID_B: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const SENDER: &str = ":1.42";

fn parse(reply: &str) -> Value {
    serde_json::from_str(reply).expect("reply is JSON")
}

fn handshake_req(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":true,\"equalCount\":false,\"sharedNativeRoster\":false,\
          \"birthClear\":false,\"nativeSelectorScope\":\"per-output\",\
          \"windowSuppression\":false,\"maximize\":true,\"fullscreen\":true,\
          \"nativeSticky\":true,\"above\":false,\"warp\":false,\
          \"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
    )
}

fn apply_settings(service: &WorldService, generation: &str) {
    let body = format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"req\":\"settings-1\",\"rev\":\"1\",\"values\":{{}}}}"
    );
    let reply: Value = serde_json::from_str(&service.discover(&body).expect("settings routes"))
        .expect("valid JSON");
    assert_eq!(reply["status"], "accepted");
}

fn discovered_service() -> (WorldService, String) {
    let service = WorldService::new();
    let discovery = parse(
        &service
            .discover(&handshake_req(ADAPTER, "req-0"))
            .expect("handshake routes"),
    );
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    apply_settings(&service, &generation);
    (service, generation)
}

fn raw_win(win: &str, x: i32, desktop: &str) -> String {
    format!(
        "{{\"win\":\"{win}\",\"out\":\"DP-1\",\"rect\":[{x},100,400,300],\
          \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\"minimized\":false,\
          \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
          \"nativeWs\":\"{desktop}\",\
          \"hostHidden\":false,\"privileged\":false,\"managed\":true,\
          \"acceptsInput\":true,\"activationInhibited\":false,\
          \"toolWindow\":false,\"owned\":false,\"transient\":false,\"modal\":false,\
          \"resizable\":true,\"above\":false,\"below\":false,\
          \"owner\":null,\"parent\":null,\"role\":null,\"appId\":null,\
          \"executable\":null,\"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\
          \"title\":null,\"type\":\"normal\",\"activationPolicy\":\"regular\",\
          \"layer\":0,\"fullscreenOwner\":null,\"fullscreenExitable\":null,\
          \"suppressionAvailable\":null,\
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}}}}"
    )
}

fn presentation() -> String {
    "{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
      \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\
      \"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\
      \"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\
      \"foregroundOut\":null}"
        .to_owned()
}

/// One complete screen plus a caller-shaped host-owned native ring:
/// `order`/`current` carry the native desktop creation/binding state,
/// `focused`/`foreground_known` carry observed foreground, `wins` carry
/// (id, native desktop, frame origin).
fn inner(
    order: &[&str],
    current: &str,
    focused: &str,
    foreground_known: bool,
    wins: &[(&str, &str, i32)],
) -> String {
    let order_str = order.join("\",\"");
    let occ = order
        .iter()
        .map(|id| format!("{{\"native\":\"{id}\",\"count\":null}}"))
        .collect::<Vec<_>>()
        .join(",");
    let win_rows = wins
        .iter()
        .map(|(win, desktop, x)| raw_win(win, *x, desktop))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{win_rows}],\"closed\":[],\
          \"native\":[{{\"out\":\"DP-1\",\"order\":[\"{order_str}\"],\
          \"current\":\"{current}\",\
          \"occupancy\":[{occ}]}}],\
          \"focused\":\"{focused}\",\"foregroundKnown\":{foreground_known},\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{},\
          \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\
          \"creationAttempts\":[],\
          \"rendererInput\":null}}",
        presentation(),
    )
}

fn capture(generation: &str, seq: u64, inner: &str) -> String {
    format!(
        "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"source\":\"src-{seq}-t\",\"sourceSeq\":\"{seq}\",\
          \"nativeInvalid\":\"{seq}\",\"report\":{inner}}}"
    )
}

fn queue_req(generation: &str, client_cmd: &str, command: &Value) -> String {
    json!({
        "v": 4,
        "gen": generation,
        "client": "panel-1",
        "clientCmd": client_cmd,
        "command": command,
    })
    .to_string()
}

fn dispatch(service: &WorldService, generation: &str, seq: u64, inner: &str) -> Value {
    parse(
        &service
            .evaluate_capture(&capture(generation, seq, inner))
            .expect("in-band"),
    )
}

fn logical_for_bind(reply: &Value, bind: &str) -> String {
    reply["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|w| w["bind"] == bind)
        .unwrap_or_else(|| panic!("no workspace bound to {bind}: {}", reply["workspaces"]))["id"]
        .as_str()
        .expect("logical id")
        .to_owned()
}

/// Splice mechanical member-attempt rows into an inner report, mirroring
/// the native member-attempt evidence that lands after focus evaluation.
fn with_attempts(inner: String, attempts: &str) -> String {
    inner.replace(
        "\"propertyAttempts\":[]",
        &format!("\"propertyAttempts\":[{attempts}]"),
    )
}
/// Splice one correlated focus-feedback row into an inner report,
/// mirroring the native focus-evidence row that lands after arrival.
fn with_feedback(inner: String, feedback: &str) -> String {
    inner.replace(
        "\"focusFeedback\":[]",
        &format!("\"focusFeedback\":[{feedback}]"),
    )
}
/// Splice one native-creation receipt row into an inner report,
/// mirroring the adapter's correlated creation evidence.
fn with_creations(inner: String, attempts: &str) -> String {
    inner.replace(
        "\"creationAttempts\":[]",
        &format!("\"creationAttempts\":[{attempts}]"),
    )
}
fn window_transfers(reply: &Value) -> Vec<&Value> {
    reply["transfers"]
        .as_array()
        .expect("transfers")
        .iter()
        .filter(|t| t["kind"] == "window" && t["win"] == "win-a")
        .collect()
}

/// Drive one service to the send-follow drain: baseline on one desktop,
/// an appended native desktop + binding, then a queued follow send of
/// win-a to the appended workspace drained with the mover still
/// natively behind (pre-arrival). Returns (service, generation, home,
/// dest, drain reply, transfer op).
fn to_send_follow_drain() -> (WorldService, String, String, String, Value, String) {
    let (service, generation) = discovered_service();
    // Fixture focus is the mover (native :906/:948 both sample it), so
    // every pre-arrival capture observes win-a focused.
    let home_inner = inner(
        &[UUID_A],
        UUID_A,
        "win-a",
        true,
        &[("win-a", UUID_A, 100), ("win-b", UUID_A, 600)],
    );
    let baseline = dispatch(&service, &generation, 1, &home_inner);
    assert_eq!(baseline["ready"], true, "baseline accepts: {baseline}");
    let home = logical_for_bind(&baseline, UUID_A);
    let appended_inner = inner(
        &[UUID_A, UUID_B],
        UUID_A,
        "win-a",
        true,
        &[("win-a", UUID_A, 100), ("win-b", UUID_A, 600)],
    );
    let appended = dispatch(&service, &generation, 2, &appended_inner);
    assert_eq!(
        appended["ready"], true,
        "appended capture accepts: {appended}"
    );
    // The appended native desktop is a trailing creation-pending
    // workspace: its binding is empty until creation settles. The send
    // targets that appended workspace.
    let dest_row = appended["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|w| w["order"] == 1)
        .unwrap_or_else(|| panic!("appended workspace row: {}", appended["workspaces"]))
        .clone();
    let dest = dest_row["id"].as_str().expect("logical id").to_owned();
    assert_ne!(dest, home, "appended desktop is a distinct logical home");
    assert_eq!(
        dest_row["bind"],
        json!(""),
        "appended workspace is still creation-pending (unbound): {dest_row}"
    );
    let queued = parse(
        &service
            .queue_command(
                SENDER,
                &queue_req(
                    &generation,
                    "1",
                    &json!({"op": "send", "win": "win-a", "out": "DP-1", "ws": dest, "follow": true}),
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(queued["outcome"], "queued");
    let drain = dispatch(&service, &generation, 3, &appended_inner);
    assert_eq!(drain["ready"], true, "send drain accepts: {drain}");
    let transfers = window_transfers(&drain);
    assert_eq!(
        transfers.len(),
        1,
        "exactly one window transfer for win-a: {}",
        drain["transfers"]
    );
    let op = transfers[0]["op"].as_str().expect("transfer op").to_owned();
    assert_eq!(
        transfers[0]["ws"],
        json!(dest),
        "transfer targets the appended destination home"
    );
    (service, generation, home, dest, drain, op)
}

#[test]
fn send_follow_focus_plans_destination_home_with_transfer_after_shown() {
    let (service, generation, home, dest, drain, op) = to_send_follow_drain();
    let request = &drain["focusRequest"];
    assert_eq!(request["target"], json!("win-a"), "focus targets the mover");
    assert_eq!(
        request["ws"],
        json!(dest),
        "focus plans against the destination home, not the pre-move home: {request}"
    );
    let shown = &request["afterShown"];
    assert_eq!(
        shown["op"],
        json!(op),
        "afterShown quotes the transfer op: {request}"
    );
    let member = shown["members"]
        .as_array()
        .expect("afterShown members")
        .iter()
        .find(|m| m["win"] == "win-a")
        .unwrap_or_else(|| panic!("afterShown carries the mover: {shown}"));
    assert_eq!(
        member["ws"],
        json!(dest),
        "afterShown member homes at the destination"
    );
    // No settle before arrival: the mover is still natively behind in
    // the drain capture, so the transfer stays retained and no switch
    // or operation settlement row names the transfer op. (The command
    // result itself settles at execution by design and proves nothing
    // about arrival, so it is not asserted here.)
    assert!(
        !drain["switchSettled"]
            .as_array()
            .expect("switchSettled")
            .iter()
            .any(|row| row["op"] == op),
        "no switch settlement before arrival: {}",
        drain["switchSettled"]
    );
    assert!(
        !drain["operationSettled"]
            .as_array()
            .expect("operationSettled")
            .iter()
            .any(|row| row["op"] == op),
        "no operation settlement before arrival: {}",
        drain["operationSettled"]
    );
    // Exact creation+bind before the move: one pending native creation
    // desire names the appended destination (out DP-1), and the
    // destination workspace row is still unbound in the drain reply.
    let creations: Vec<&Value> = drain["nativeCreations"]
        .as_array()
        .expect("nativeCreations")
        .iter()
        .filter(|row| row["ws"] == dest)
        .collect();
    assert_eq!(
        creations.len(),
        1,
        "exactly one creation desire for the destination: {}",
        drain["nativeCreations"]
    );
    assert_eq!(
        creations[0]["out"],
        json!("DP-1"),
        "creation targets the mover output"
    );
    assert!(
        creations[0]["op"].as_str().is_some_and(|op| !op.is_empty()),
        "creation carries its op identity"
    );
    let dest_row = drain["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|w| w["id"] == dest)
        .unwrap_or_else(|| panic!("destination row: {}", drain["workspaces"]));
    assert_eq!(
        dest_row["bind"],
        json!(""),
        "destination is still creation-pending at the send drain: {dest_row}"
    );
    // Arrival, landing after the focus evaluation: the mover is
    // observed natively on the destination desktop with the accepted
    // member attempt, and the correlated focus evidence proves the mover
    // foreground with a readiness bracket at the destination home. Only
    // now may the command settle, and it must settle complete+proven.
    let feedback = format!(
        "{{\"id\":\"{}\",\"target\":\"win-a\",\"rev\":\"{}\",\
          \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"reason\":\"\",\
          \"pre\":\"win-b\",\"post\":\"win-a\",\"foregroundProven\":true,\
          \"arrival\":[{{\"win\":\"win-a\",\"obs\":\"4\",\
          \"before\":{{\"obs\":\"4\",\"lifetimeMatched\":true,\
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"minimized\":false,\"hostHidden\":false}},\
          \"kind\":\"native\",\
          \"home\":{{\"kind\":\"native\",\"out\":\"DP-1\",\"ws\":\"{UUID_B}\"}},\
          \"frame\":[100,100,400,300],\
          \"after\":{{\"obs\":\"4\",\"lifetimeMatched\":true,\
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"minimized\":false,\"hostHidden\":false}}}}],\
          \"attemptMs\":\"0\"}}",
        request["id"].as_str().expect("focus id"),
        request["rev"].as_str().expect("focus rev"),
    );
    // The correlated creation receipt binds the destination desktop
    // (stamped at the drain clock, between mint and this capture), and
    // the member attempt carries its pre/post homes.
    let creation = format!(
        "{{\"op\":\"{}\",\"ws\":\"{dest}\",\"out\":\"DP-1\",\"setter\":\"accepted\",\
          \"before\":[\"{UUID_A}\"],\"after\":[\"{UUID_A}\",\"{UUID_B}\"],\
          \"created\":\"{UUID_B}\",\"attemptMs\":\"{}\"}}",
        creations[0]["op"].as_str().expect("creation op"),
        drain["clock"].as_str().expect("drain clock"),
    );
    let arrival_inner = with_creations(
        with_feedback(
            with_attempts(
                inner(
                    &[UUID_A, UUID_B],
                    UUID_B,
                    "win-a",
                    true,
                    &[("win-a", UUID_B, 100), ("win-b", UUID_A, 600)],
                ),
                &format!(
                    "{{\"kind\":\"member\",\"op\":\"{op}\",\"win\":\"win-a\",\
                      \"out\":\"DP-1\",\"ws\":\"{dest}\",\
                      \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\
                      \"pre\":{{\"kind\":\"logical\",\"out\":\"DP-1\",\"ws\":\"{home}\"}},\
                      \"post\":{{\"kind\":\"logical\",\"out\":\"DP-1\",\"ws\":\"{dest}\"}},\
                      \"attemptMs\":0}}"
                ),
            ),
            &feedback,
        ),
        &creation,
    );
    let arrived = dispatch(&service, &generation, 4, &arrival_inner);
    assert_eq!(arrived["ready"], true, "arrival capture accepts: {arrived}");
    assert!(
        arrived["focusRequest"].is_null() || arrived["focusRequest"]["target"] == "win-a",
        "arrival never retargets focus away from the mover: {}",
        arrived["focusRequest"]
    );
    let settled: Vec<&Value> = arrived["switchSettled"]
        .as_array()
        .expect("switchSettled")
        .iter()
        .filter(|row| row["op"] == op)
        .collect();
    assert_eq!(
        settled.len(),
        1,
        "exactly one terminal switch row for the transfer op: {}",
        arrived["switchSettled"]
    );
    assert_eq!(
        settled[0]["outcome"],
        json!("complete"),
        "the follow settles complete once arrival proves: {}",
        settled[0]
    );
    assert_eq!(
        settled[0]["focus"],
        json!("proven"),
        "focus is proven by the correlated feedback: {}",
        settled[0]
    );
    assert_eq!(
        settled[0]["failed"],
        json!([]),
        "no member failed: {}",
        settled[0]
    );
}
#[test]
fn send_follow_perturbation_never_focuses_wrong_window_nor_completes() {
    // Separate perturbation control (no such foreign activation was
    // captured in the native log): foreign activation (observed focus
    // moves to win-b) plus a native workspace switch (current moves to
    // the destination desktop) mid-flight, with the mover still
    // natively behind. Neither half of the perturbation is arrival, so
    // focus must never target the foreign window and the command must
    // never settle as complete.
    let (service, generation, _home, _dest, drain, op) = to_send_follow_drain();
    assert_eq!(drain["focusRequest"]["target"], json!("win-a"));
    let perturbed_inner = inner(
        &[UUID_A, UUID_B],
        UUID_B,
        "win-b",
        true,
        &[("win-a", UUID_A, 100), ("win-b", UUID_A, 600)],
    );
    let perturbed = dispatch(&service, &generation, 4, &perturbed_inner);
    assert_eq!(
        perturbed["ready"], true,
        "perturbed capture accepts: {perturbed}"
    );
    let request = &perturbed["focusRequest"];
    assert!(
        request.is_null() || request["target"] == json!("win-a"),
        "perturbation never plans focus on the foreign window: {request}"
    );
    // Neither half of the perturbation is arrival: the transfer stays
    // retained and no settlement row names the transfer op, so the
    // send can never falsely complete.
    assert_eq!(
        window_transfers(&perturbed).len(),
        1,
        "transfer stays retained under perturbation: {}",
        perturbed["transfers"]
    );
    for lane in ["switchSettled", "operationSettled"] {
        assert!(
            !perturbed[lane]
                .as_array()
                .expect("settlement lane")
                .iter()
                .any(|row| row["op"] == op),
            "no {lane} row for the transfer op under perturbation: {}",
            perturbed[lane]
        );
    }
}

fn selection_capture(current: &str, focused: &str) -> String {
    inner(
        &[UUID_A, UUID_B],
        current,
        focused,
        true,
        &[
            ("win-source", UUID_A, 100),
            ("win-t", UUID_B, 100),
            ("win-c", UUID_B, 600),
        ],
    )
}

fn to_selection_focus_drain() -> (WorldService, String, Value) {
    let (service, generation) = discovered_service();
    let source = selection_capture(UUID_A, "win-source");
    let seed = dispatch(&service, &generation, 1, &source);
    assert_eq!(seed["ready"], true, "two-desktop seed accepts: {seed}");
    let home = logical_for_bind(&seed, UUID_A);
    let dest = logical_for_bind(&seed, UUID_B);
    assert_ne!(home, dest);
    for (seq, current, focused) in [
        (2, UUID_B, "win-c"),
        (3, UUID_B, "win-t"),
        (4, UUID_A, "win-source"),
    ] {
        let reply = dispatch(
            &service,
            &generation,
            seq,
            &selection_capture(current, focused),
        );
        assert_eq!(reply["ready"], true, "seed capture {seq} accepts: {reply}");
    }
    let queued = parse(
        &service
            .queue_command(
                SENDER,
                &queue_req(
                    &generation,
                    "1",
                    &json!({"op": "select-workspace", "out": "DP-1", "ws": dest}),
                ),
            )
            .expect("selection routes"),
    );
    assert_eq!(queued["outcome"], "queued", "{queued}");
    let issuing = dispatch(&service, &generation, 5, &source);
    assert_eq!(issuing["ready"], true, "selection drain accepts: {issuing}");
    assert_eq!(
        issuing["focusRequest"]["target"], "win-t",
        "selection restores the remembered destination window: {issuing}"
    );
    assert_eq!(issuing["focusRequest"]["ws"], dest, "{issuing}");
    (service, generation, issuing)
}

fn selection_view_readback(issuing: &Value, focused: &str) -> String {
    let request = &issuing["focusRequest"];
    let op = &request["afterShown"]["op"];
    assert!(
        issuing["transfers"]
            .as_array()
            .expect("transfers")
            .iter()
            .any(|row| row["op"] == *op && row["kind"] == "view" && row["ws"] == request["ws"]),
        "issuing reply carries the selection's native view operation: {issuing}"
    );
    let source = json!({"kind": "native", "out": "DP-1", "ws": UUID_A});
    let destination = json!({"kind": "native", "out": "DP-1", "ws": UUID_B});
    let readback = json!({
        "kind": "view",
        "op": op,
        "out": "DP-1",
        "ws": request["ws"],
        "disposition": {"kind": "called", "setter": "accepted"},
        "pre": source,
        "post": destination,
        "base": source,
        "attemptMs": issuing["clock"],
    });
    with_attempts(selection_capture(UUID_B, focused), &readback.to_string())
}

#[test]
fn selection_focus_survives_own_view_switch_fallback() {
    let (service, generation, issuing) = to_selection_focus_drain();
    let request = &issuing["focusRequest"];
    let id = request["id"].as_str().expect("issuing focus id");
    let rev = request["rev"].as_str().expect("issuing focus revision");
    let op = request["afterShown"]["op"].as_str().expect("selection op");

    // Do not apply the issuing focus request. Only the view setter ran,
    // and the host restored C while showing the destination.
    let snapshot = selection_view_readback(&issuing, "win-c");
    let incorporated_rev = issuing["rev"].as_str().expect("incorporated revision");
    assert_eq!(incorporated_rev, rev);
    let readback = with_empty_focus_interval(snapshot.clone(), incorporated_rev);
    let envelope = capture(&generation, 6, &readback);
    let wire = service.evaluate_capture(&envelope).expect("in-band");
    let shown = parse(&wire);
    assert_ne!(shown["obs"], issuing["obs"], "fresh view readback: {shown}");
    assert_eq!(
        service.evaluate_capture(&envelope).expect("replay"),
        wire,
        "the same capture must not dispatch the interval twice"
    );
    let changed = dispatch(&service, &generation, 6, &snapshot);
    assert_eq!(changed["kind"], "source-reused", "interval binds raw bytes");
    assert_eq!(shown["ready"], true, "view readback accepts: {shown}");
    let retained = &shown["focusRequest"];
    assert_eq!(
        (retained["id"].as_str(), retained["rev"].as_str()),
        (Some(id), Some(rev)),
        "own view switch must retain T across source → C foreground; \
         capture (including view attempt, no focus feedback): {readback}; reply: {shown}"
    );
    assert_eq!(retained["target"], "win-t", "{retained}");
    for reply in [&issuing, &shown] {
        assert!(
            !reply["switchSettled"]
                .as_array()
                .expect("switchSettled")
                .iter()
                .any(|row| row["op"] == op && row["outcome"] == "complete"),
            "selection cannot complete before T's arrival is proved: {}",
            reply["switchSettled"]
        );
    }

    let feedback = selection_arrival_feedback(retained, &shown);
    let arrival = with_empty_focus_interval(
        with_feedback(selection_capture(UUID_B, "win-t"), &feedback.to_string()),
        shown["rev"].as_str().expect("incorporated shown revision"),
    );
    let arrived = dispatch(&service, &generation, 7, &arrival);
    assert_eq!(arrived["ready"], true, "arrival capture accepts: {arrived}");
    assert!(arrived["focusRequest"].is_null(), "{arrived}");
    let settled: Vec<_> = arrived["switchSettled"]
        .as_array()
        .expect("switchSettled")
        .iter()
        .filter(|row| row["op"] == op)
        .collect();
    assert_eq!(settled.len(), 1, "one terminal selection row: {arrived}");
    assert_eq!(settled[0]["outcome"], "complete", "{arrived}");
    assert_eq!(settled[0]["focus"], "proven", "{arrived}");
    assert_eq!(settled[0]["failed"], json!([]), "{arrived}");
}

fn with_empty_focus_interval(inner: String, incorporated_rev: &str) -> String {
    let changes = json!({"after": incorporated_rev, "events": []});
    inner.replace(
        "\"focusChanges\":null",
        &format!("\"focusChanges\":{changes}"),
    )
}

fn selection_arrival_feedback(request: &Value, shown: &Value) -> Value {
    // KWin brackets each frozen member at the shown reply's observation,
    // then submits the correlated attempt on the following capture.
    let sample = json!({
        "obs": shown["obs"],
        "lifetimeMatched": true,
        "visibility": {"visible": true, "suppressionOwned": false},
        "minimized": false,
        "hostHidden": false,
    });
    let members = request["afterShown"]["members"]
        .as_array()
        .expect("members");
    assert!(members.iter().any(|member| member["win"] == "win-t"));
    let arrival: Vec<_> = members
        .iter()
        .map(|member| {
            assert_eq!(member["out"], "DP-1");
            assert_eq!(member["ws"], request["ws"]);
            let frame = if member["frame"].is_null() {
                match member["win"].as_str().expect("member identity") {
                    "win-t" => json!([100, 100, 400, 300]),
                    "win-c" => json!([600, 100, 400, 300]),
                    other => panic!("unexpected destination member: {other}"),
                }
            } else {
                member["frame"].clone()
            };
            json!({
                "win": member["win"],
                "obs": shown["obs"],
                "kind": "native",
                "home": {"kind": "native", "out": "DP-1", "ws": UUID_B},
                "frame": frame,
                "before": sample,
                "after": sample,
            })
        })
        .collect();
    json!({
        "id": request["id"],
        "target": request["target"],
        "rev": request["rev"],
        "disposition": {"kind": "called", "setter": "accepted"},
        "reason": "",
        "pre": "win-c",
        "post": "win-t",
        "foregroundProven": true,
        "arrival": arrival,
        "attemptMs": shown["clock"],
    })
}

#[test]
fn selection_focus_snapshot_only_does_not_prove_own_view_switch_fallback() {
    let (service, generation, issuing) = to_selection_focus_drain();
    let readback = selection_view_readback(&issuing, "win-c");
    let shown = dispatch(&service, &generation, 6, &readback);
    assert_eq!(shown["ready"], true, "snapshot readback accepts: {shown}");
    assert!(
        shown["focusRequest"].is_null(),
        "unattributed foreground change: {shown}"
    );
    let op = &issuing["focusRequest"]["afterShown"]["op"];
    assert!(
        !shown["switchSettled"]
            .as_array()
            .expect("switchSettled")
            .iter()
            .any(|row| row["op"] == *op && row["outcome"] == "complete"),
        "snapshot-only view readback cannot complete selection: {shown}"
    );
}
