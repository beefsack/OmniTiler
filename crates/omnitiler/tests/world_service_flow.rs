//! Service-level world flow: discovery/bootstrap binding, full
//! report/reply round-trips through [`omnitiler::world_service`],
//! duplicate/consumed/ordering behavior over JSON, overbound
//! transactional safety, and generation freshness. Offline only: no D-Bus,
//! no KWin.

use omnitiler::world_service::{WorldService, WorldServiceError};

fn window_json(id: &str, ws: &str, x: i32) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"out\",\"ws\":\"{ws}\",\"rect\":[{x},0,200,200],\
         \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
         \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
         \"fixedSuppress\":false,\
         \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}}}"
    )
}

fn report_json(
    generation: &str,
    obs: u64,
    req_seq: u64,
    cmd_seq: Option<u64>,
    command: Option<&str>,
) -> String {
    let (cmd_seq_json, command_json) = match (cmd_seq, command) {
        (Some(seq), Some(cmd)) => (format!("\"{seq}\""), cmd.to_owned()),
        (None, None) => ("null".to_owned(), "null".to_owned()),
        _ => panic!("command and cmdSeq pair up"),
    };
    format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
         \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"req-{req_seq}\",\
         \"cmdSeq\":{cmd_seq_json},\"command\":{command_json},\
         \"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"out\",\"order\":[\"ws1\",\"ws2\"],\
         \"current\":\"ws1\",\"complete\":true,\"rect\":null,\"primary\":false}}],\"bindings\":[],\
         \"domains\":[\
         {{\"id\":\"out\",\"ws\":\"ws1\",\"bounds\":[0,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"win-a\",\"readable\":true,\"hold\":\"\",\
         \"adjacent\":{{}},\"windows\":[{}],\"closed\":[]}},\
         {{\"id\":\"out\",\"ws\":\"ws2\",\"bounds\":[0,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\
         \"adjacent\":{{}},\"windows\":[],\"closed\":[]}}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}",
        window_json("win-a", "ws1", 0)
    )
}

fn generation_of(reply_json: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(reply_json).expect("valid JSON");
    parsed["gen"].as_str().expect("gen field").to_owned()
}

#[test]
fn discovery_binds_generation_before_any_ready_reply() {
    let service = WorldService::new();
    let generation = service.generation();
    assert!(!generation.is_empty());
    let discovery = service.discovery_body("adapter-1", "req-0");
    let parsed: serde_json::Value = serde_json::from_str(&discovery).expect("valid JSON");
    assert_eq!(parsed["ready"], false);
    assert_eq!(parsed["rev"], "0");
    assert_eq!(parsed["gen"].as_str(), Some(generation.as_str()));
    assert_eq!(parsed["windows"].as_array().map(Vec::len), Some(0));
    // A stale generation never yields ready state.
    let stale = report_json("gen-stale", 1, 1, None, None);
    let reply = service.evaluate_world_request(&stale).expect("in-band");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "generation-mismatch");
    assert_eq!(parsed["gen"].as_str(), Some(generation.as_str()));
}

#[test]
fn full_flow_duplicate_consumed_and_ordering_over_json() {
    let service = WorldService::new();
    let generation = service.generation();
    // Accepted observation-only report: ready full-state reply.
    let first = report_json(&generation, 1, 1, None, None);
    let reply = service.evaluate_world_request(&first).expect("accepted");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["ready"], true);
    assert_eq!(parsed["rev"], "1");
    assert_eq!(parsed["obs"], "1");
    // Identical latest duplicate: byte-identical reply, no new revision.
    let duplicate = service.evaluate_world_request(&first).expect("duplicate");
    assert_eq!(duplicate, reply);
    // Fresh send command retains a transfer.
    let send = "{\"op\":\"send\",\"win\":\"win-a\",\"out\":\"out\",\"ws\":\"ws2\",\"follow\":true}";
    let commanded = report_json(&generation, 2, 2, Some(7), Some(send));
    let reply = service
        .evaluate_world_request(&commanded)
        .expect("send accepted");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["rev"], "2");
    assert_eq!(parsed["transfers"].as_array().map(Vec::len), Some(1));
    // Same command sequence again: observation reconciles, never re-executes.
    let replay = report_json(&generation, 3, 3, Some(7), Some(send));
    let reply = service.evaluate_world_request(&replay).expect("consumed");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["consumed"], true);
    assert_eq!(parsed["transfers"].as_array().map(Vec::len), Some(1));
    // Reused request sequence with different input is invalid: same
    // sequence as the latest accepted request but changed bytes.
    let mut changed: serde_json::Value = serde_json::from_str(&first).expect("valid");
    changed["gesture"] = serde_json::Value::Bool(true);
    changed["reqSeq"] = serde_json::Value::String("3".to_owned());
    changed["req"] = serde_json::Value::String("req-3".to_owned());
    let reply = service
        .evaluate_world_request(&changed.to_string())
        .expect("in-band");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["kind"], "request-reused");
    // Older request sequence is superseded, never executed.
    let older = report_json(&generation, 4, 2, None, None);
    let reply = service.evaluate_world_request(&older).expect("in-band");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["kind"], "request-superseded");
}

#[test]
fn overbound_request_fails_closed_without_touching_backend() {
    let service = WorldService::new();
    let generation = service.generation();
    let big = "x".repeat(4 * 1024 * 1024 + 1);
    match service.evaluate_world_request(&big) {
        Err(WorldServiceError::Oversized) => {}
        other => panic!("expected oversized, got {other:?}"),
    }
    // Backend untouched: a valid report still starts at revision 1.
    let first = report_json(&generation, 1, 1, None, None);
    let reply = service.evaluate_world_request(&first).expect("accepted");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["rev"], "1");
}

#[test]
fn constructions_mint_fresh_generations() {
    let first = WorldService::new();
    let second = WorldService::new();
    assert_ne!(first.generation(), second.generation());
    // Reports do not cross generations.
    let cross = report_json(&first.generation(), 1, 1, None, None);
    let reply = second.evaluate_world_request(&cross).expect("in-band");
    assert_eq!(generation_of(&reply), second.generation());
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["kind"], "generation-mismatch");
}

#[test]
fn geometric_exhaustion_holds_truthfully_over_json() {
    // Best-effort admission over JSON (selected D2/D3): fourteen fresh
    // overlapping 200x200 windows in one 800x600/gap-8 domain no longer
    // hold — largest-fallback packing tiles all fourteen in one atomic
    // roster, so every row carries an allocation under a controlled
    // scope. Recovery through fewer windows succeeds identically.
    let service = WorldService::new();
    let generation = service.generation();
    let windows: Vec<String> = (0..14)
        .map(|i| {
            format!(
                "{{\"win\":\"win-{i:06}\",\"out\":\"out\",\"ws\":\"ws\",\"rect\":[0,0,200,200],\
                 \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
                 \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
                 \"fixedSuppress\":false,\
                 \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}}}"
            )
        })
        .collect();
    let packed_request = format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
         \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\"command\":null,\
         \"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"out\",\"order\":[\"ws\"],\"current\":\"ws\",\"complete\":true,\"rect\":null,\"primary\":false}}],\
         \"bindings\":[],\
         \"domains\":[{{\"id\":\"out\",\"ws\":\"ws\",\"bounds\":[0,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\"adjacent\":{{}},\
         \"windows\":[{}],\"closed\":[]}}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}",
        windows.join(",")
    );
    let reply = service
        .evaluate_world_request(&packed_request)
        .expect("packed");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["ready"], true);
    assert_eq!(parsed["scopes"].as_array().map(Vec::len), Some(1));
    assert_eq!(parsed["scopes"][0]["auth"], "controlled");
    assert_eq!(parsed["windows"].as_array().map(Vec::len), Some(14));
    assert!(
        parsed["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| !w["alloc"].is_null()),
        "best-effort packs every row with an allocation"
    );
    // Stable replay: the same fourteen converge exactly on the same
    // backend, staying controlled with every allocation intact.
    let fitting_windows: Vec<String> = windows.clone();
    let fitting = format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
         \"obs\":\"2\",\"reqSeq\":\"2\",\"req\":\"req-2\",\"cmdSeq\":null,\"command\":null,\
         \"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"out\",\"order\":[\"ws\"],\"current\":\"ws\",\"complete\":true,\"rect\":null,\"primary\":false}}],\
         \"bindings\":[],\
         \"domains\":[{{\"id\":\"out\",\"ws\":\"ws\",\"bounds\":[0,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\"adjacent\":{{}},\
         \"windows\":[{}],\"closed\":[]}}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}",
        fitting_windows.join(",")
    );
    let reply = service.evaluate_world_request(&fitting).expect("recovered");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["scopes"][0]["auth"], "controlled");
    assert!(
        parsed["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| !w["alloc"].is_null()),
        "recovered rows all carry allocations"
    );
}

#[test]
fn tiny_domain_commits_complete_mixed_roster_over_json() {
    // Genuinely tiny domain (10x10/gap-8, six arrivals): best-effort tiles
    // four and capacity-floats two in one atomic commit. The inventory
    // stays complete over JSON (six rows, none dropped, no holds): tiled
    // rows carry allocations while float rows carry none with untouched
    // frame authority, never a constraint-clamp hold.
    let service = WorldService::new();
    let generation = service.generation();
    let windows: Vec<String> = (0..6)
        .map(|i| {
            format!(
                "{{\"win\":\"win-{i:06}\",\"out\":\"tiny\",\"ws\":\"ws\",\"rect\":[0,0,50,50],\
                 \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
                 \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
                 \"fixedSuppress\":false,\
                 \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}}}"
            )
        })
        .collect();
    let request = format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
         \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\"command\":null,\
         \"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"tiny\",\"order\":[\"ws\"],\"current\":\"ws\",\"complete\":true,\"rect\":null,\"primary\":false}}],\
         \"bindings\":[],\
         \"domains\":[{{\"id\":\"tiny\",\"ws\":\"ws\",\"bounds\":[0,0,10,10],\"gap\":8,\
         \"outer\":0,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\"adjacent\":{{}},\
         \"windows\":[{}],\"closed\":[]}}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}",
        windows.join(",")
    );
    let reply = service.evaluate_world_request(&request).expect("mixed");
    let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
    assert_eq!(parsed["ready"], true);
    assert_eq!(parsed["scopes"].as_array().map(Vec::len), Some(1));
    assert_eq!(parsed["scopes"][0]["auth"], "controlled");
    let rows = parsed["windows"].as_array().expect("windows");
    assert_eq!(rows.len(), 6, "complete inventory, never silent absence");
    assert_eq!(
        rows.iter().filter(|w| !w["alloc"].is_null()).count(),
        4,
        "four tiles carry allocations"
    );
    let floats: Vec<&serde_json::Value> = rows.iter().filter(|w| w["alloc"].is_null()).collect();
    assert_eq!(floats.len(), 2, "two capacity floats ride slotless");
    assert!(
        floats.iter().all(|w| w["frame_auth"] == "untouched"
            && w["frame_hold"] != "constraint-clamp"
            && w["member_auth"] != "hold"),
        "float rows authorize no writes and never hold: {floats:?}"
    );
    assert!(
        rows.iter()
            .all(|w| w["frame_auth"] != "hold" && w["frame_hold"] != "constraint-clamp"),
        "no holds anywhere in the mixed roster"
    );
}
