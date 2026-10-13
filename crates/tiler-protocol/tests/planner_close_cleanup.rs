#![forbid(unsafe_code)]
//! D23 close-versus-minimize wire distinction (`closed_windows`).
//!
//! - Absent `closed_windows` parses as empty (legacy requests byte-identical,
//!   minimize-on-omission preserved).
//! - Closed ids validate like live observations: opaque shape, no duplicates,
//!   and no contradictory live+closed row, all fail-closed as
//!   `snapshot-invalid` without new vocabulary.
//! - End to end through the real [`Planner`](tiler_protocol::planner_protocol::Planner):
//!   a minimized close with the report retires the slot-only session, while a
//!   rejected (contradictory) close mutates nothing and the retry delivers.

use tiler_protocol::planner_protocol::Planner;

fn window_entry(window: &str) -> serde_json::Value {
    serde_json::json!({
        "window": window,
        "output": "out-1",
        "workspace": "ws-1",
        "rect": {"x": 0, "y": 0, "w": 10, "h": 10},
    })
}

fn request(
    correlation: &str,
    focused: &str,
    windows: Vec<serde_json::Value>,
    extra: serde_json::Value,
) -> String {
    let mut body = serde_json::json!({
        "v": 1,
        "correlation_id": correlation,
        "owner": "owner-1",
        "generation": "gen-1",
        "revision": 0,
        "fingerprint": 7,
        "domain": {
            "output": "out-1",
            "workspace": "ws-1",
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
            "gap": 0,
            "outer_gap": 0,
        },
        "focused_window": focused,
        "windows": windows,
        "command": {"op": "reconcile"},
    });
    for (key, value) in extra.as_object().expect("extra is an object") {
        body[key] = value.clone();
    }
    body.to_string()
}

fn reply(planner: &mut Planner, request: &str) -> serde_json::Value {
    serde_json::from_str(&planner.evaluate(request)).expect("reply is JSON")
}

fn no_extra() -> serde_json::Value {
    serde_json::json!({})
}

#[test]
fn absent_closed_windows_stays_compatible_and_tiles() {
    let mut planner = Planner::new();
    // Legacy shape: no `closed_windows` field at all.
    let first = reply(
        &mut planner,
        &request(
            "close-compat-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(first["outcome"], "planned", "{first}");
    assert_eq!(planner.retained_domains(), 1);
}

#[test]
fn invalid_closed_id_refuses_fail_closed() {
    let mut planner = Planner::new();
    let seeded = reply(
        &mut planner,
        &request(
            "close-valid-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");

    let bad = reply(
        &mut planner,
        &request(
            "close-valid-2",
            "",
            vec![],
            serde_json::json!({"closed_windows": ["bad id"]}),
        ),
    );
    assert_eq!(bad["outcome"], "rejected", "{bad}");
    assert_eq!(bad["kind"], "snapshot-invalid", "{bad}");
    assert_eq!(bad["detail"], "observed-window-invalid", "{bad}");
}

#[test]
fn duplicate_closed_id_refuses_fail_closed() {
    let mut planner = Planner::new();
    let seeded = reply(
        &mut planner,
        &request(
            "close-dup-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");

    let dup = reply(
        &mut planner,
        &request(
            "close-dup-2",
            "",
            vec![],
            serde_json::json!({"closed_windows": ["win-a", "win-a"]}),
        ),
    );
    assert_eq!(dup["outcome"], "rejected", "{dup}");
    assert_eq!(dup["kind"], "snapshot-invalid", "{dup}");
    assert_eq!(dup["detail"], "duplicate-window", "{dup}");
}

#[test]
fn contradictory_live_and_closed_refuses_fail_closed() {
    let mut planner = Planner::new();
    let seeded = reply(
        &mut planner,
        &request(
            "close-contra-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");

    // One id cannot be both observed and reported closed.
    let contra = reply(
        &mut planner,
        &request(
            "close-contra-2",
            "win-a",
            vec![window_entry("win-a")],
            serde_json::json!({"closed_windows": ["win-a"]}),
        ),
    );
    assert_eq!(contra["outcome"], "rejected", "{contra}");
    assert_eq!(contra["kind"], "snapshot-invalid", "{contra}");
    assert_eq!(contra["detail"], "duplicate-window", "{contra}");
}

#[test]
fn unknown_closed_shape_refuses_fail_closed() {
    let mut planner = Planner::new();
    let seeded = reply(
        &mut planner,
        &request(
            "close-shape-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");

    let unknown = reply(
        &mut planner,
        &request(
            "close-shape-2",
            "",
            vec![],
            serde_json::json!({"closed_windowz": ["win-a"]}),
        ),
    );
    assert_eq!(unknown["outcome"], "rejected", "{unknown}");
    assert_eq!(unknown["kind"], "unknown-field", "{unknown}");
}

#[test]
fn minimized_close_with_report_retires_sole_session() {
    let mut planner = Planner::new();
    let seeded = reply(
        &mut planner,
        &request(
            "close-e2e-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");

    // Sole minimize as a bare omission: the slot-only session is retained.
    let minimized = reply(
        &mut planner,
        &request("close-e2e-2", "", vec![], no_extra()),
    );
    assert_eq!(minimized["outcome"], "planned", "{minimized}");
    assert_eq!(planner.retained_domains(), 1, "slot retains the session");

    // The minimized close with the report retires the session on the turn.
    let closed = reply(
        &mut planner,
        &request(
            "close-e2e-3",
            "",
            vec![],
            serde_json::json!({"closed_windows": ["win-a"]}),
        ),
    );
    assert_eq!(closed["outcome"], "planned", "{closed}");
    assert_eq!(
        planner.retained_domains(),
        0,
        "forgotten slot retires the session"
    );
}

#[test]
fn rejected_close_mutates_nothing_and_retry_delivers() {
    let mut planner = Planner::new();
    let seeded = reply(
        &mut planner,
        &request(
            "close-retry-1",
            "win-a",
            vec![window_entry("win-a")],
            no_extra(),
        ),
    );
    assert_eq!(seeded["outcome"], "planned", "{seeded}");
    let minimized = reply(
        &mut planner,
        &request("close-retry-2", "", vec![], no_extra()),
    );
    assert_eq!(minimized["outcome"], "planned", "{minimized}");
    assert_eq!(planner.retained_domains(), 1);

    // Contradictory close refuses without mutating: the slot survives.
    let contra = reply(
        &mut planner,
        &request(
            "close-retry-3",
            "win-b",
            vec![window_entry("win-b")],
            serde_json::json!({"closed_windows": ["win-b"]}),
        ),
    );
    assert_eq!(contra["outcome"], "rejected", "{contra}");
    assert_eq!(planner.retained_domains(), 1, "rejected close retains");

    // Retry with the close retained delivers and retires.
    let closed = reply(
        &mut planner,
        &request(
            "close-retry-4",
            "",
            vec![],
            serde_json::json!({"closed_windows": ["win-a"]}),
        ),
    );
    assert_eq!(closed["outcome"], "planned", "{closed}");
    assert_eq!(
        planner.retained_domains(),
        0,
        "retried close retires the session"
    );
}
