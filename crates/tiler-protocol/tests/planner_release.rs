#![forbid(unsafe_code)]
//! Explicit release-domain route: one domain drops with no geometry while an
//! independent domain keeps planning, then an ordinary fresh reconcile
//! re-adopts the released domain's current geometry.

use tiler_protocol::planner_protocol::Planner;

fn window_entry(
    window: &str,
    output: &str,
    workspace: &str,
    rect: (i32, i32, i32, i32),
) -> serde_json::Value {
    serde_json::json!({
        "window": window,
        "output": output,
        "workspace": workspace,
        "rect": {"x": rect.0, "y": rect.1, "w": rect.2, "h": rect.3},
    })
}

fn request(
    correlation: &str,
    output: &str,
    workspace: &str,
    focused: &str,
    windows: Vec<serde_json::Value>,
    command: serde_json::Value,
) -> String {
    serde_json::json!({
        "v": 1,
        "correlation_id": correlation,
        "owner": "owner-1",
        "generation": "gen-1",
        "revision": 0,
        "fingerprint": 7,
        "domain": {
            "output": output,
            "workspace": workspace,
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
            "gap": 0,
            "outer_gap": 0,
        },
        "focused_window": focused,
        "windows": windows,
        "command": command,
    })
    .to_string()
}

fn reply(planner: &mut Planner, request: &str) -> serde_json::Value {
    serde_json::from_str(&planner.evaluate(request)).expect("reply is JSON")
}

fn geometry_by_window(reply: &serde_json::Value) -> Vec<(String, (i32, i32, i32, i32))> {
    let mut entries: Vec<(String, (i32, i32, i32, i32))> = reply["desired_geometry"]
        .as_array()
        .expect("planned geometry present")
        .iter()
        .map(|entry| {
            let rect = &entry["rect"];
            (
                entry["window"].as_str().expect("window").to_owned(),
                (
                    rect["x"].as_i64().expect("x") as i32,
                    rect["y"].as_i64().expect("y") as i32,
                    rect["w"].as_i64().expect("w") as i32,
                    rect["h"].as_i64().expect("h") as i32,
                ),
            )
        })
        .collect();
    entries.sort();
    entries
}

#[test]
fn release_then_fresh_reconcile_readopts_current_geometry() {
    let mut planner = Planner::new();
    let current = vec![
        window_entry("win-a1", "out-1", "ws-1", (0, 0, 600, 800)),
        window_entry("win-a2", "out-1", "ws-1", (600, 0, 600, 800)),
    ];
    let seeded = reply(
        &mut planner,
        &request(
            "rel-wire-1",
            "out-1",
            "ws-1",
            "win-a2",
            current.clone(),
            serde_json::json!({"op": "reconcile"}),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");
    let seeded_geometry = geometry_by_window(&seeded);

    // The independent domain plans before the release.
    let away = reply(
        &mut planner,
        &request(
            "rel-wire-2",
            "out-2",
            "ws-1",
            "win-b1",
            vec![window_entry("win-b1", "out-2", "ws-1", (0, 0, 1200, 800))],
            serde_json::json!({"op": "reconcile"}),
        ),
    );
    assert_eq!(away["outcome"], "planned", "{away}");
    let away_geometry = geometry_by_window(&away);

    // Release carries the exact domain's current observation and writes
    // nothing: no geometry, no focus, no revision.
    let released = reply(
        &mut planner,
        &request(
            "rel-wire-3",
            "out-1",
            "ws-1",
            "win-a2",
            current.clone(),
            serde_json::json!({"op": "release-domain"}),
        ),
    );
    assert_eq!(released["outcome"], "released", "{released}");
    assert_eq!(released["kind"], "release-domain", "{released}");
    assert_eq!(released["detail"]["kind"], "release-domain", "{released}");
    assert!(released.get("desired_geometry").is_none(), "{released}");
    assert!(released.get("desired_focus").is_none(), "{released}");
    assert!(released.get("float_geometry").is_none(), "{released}");
    assert!(released.get("base_revision").is_none(), "{released}");
    assert!(released.get("preconditions").is_none(), "{released}");
    assert!(released.get("operation").is_none(), "{released}");

    // Unknown domains release idempotently with the same contract.
    let unknown = reply(
        &mut planner,
        &request(
            "rel-wire-4",
            "out-9",
            "ws-9",
            "",
            vec![],
            serde_json::json!({"op": "release-domain"}),
        ),
    );
    assert_eq!(unknown["outcome"], "released", "{unknown}");
    assert_eq!(unknown["kind"], "release-domain", "{unknown}");

    // The independent domain is unaffected by the other domain's release.
    let away_again = reply(
        &mut planner,
        &request(
            "rel-wire-5",
            "out-2",
            "ws-1",
            "win-b1",
            vec![window_entry("win-b1", "out-2", "ws-1", (0, 0, 1200, 800))],
            serde_json::json!({"op": "reconcile"}),
        ),
    );
    assert_eq!(away_again["outcome"], "planned", "{away_again}");
    assert_eq!(geometry_by_window(&away_again), away_geometry);

    // An ordinary fresh reconcile re-adopts the released domain's current
    // geometry through the existing fit.
    let readapted = reply(
        &mut planner,
        &request(
            "rel-wire-6",
            "out-1",
            "ws-1",
            "win-a2",
            current.clone(),
            serde_json::json!({"op": "reconcile"}),
        ),
    );
    assert_eq!(readapted["outcome"], "planned", "{readapted}");
    assert_eq!(geometry_by_window(&readapted), seeded_geometry);
}
