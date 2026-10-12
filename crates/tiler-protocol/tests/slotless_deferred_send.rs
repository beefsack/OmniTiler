//! D28 slotless deferred transfer over the real Linux planner route.
//!
//! Production path on ONE retained `Planner` instance (no fresh-engine
//! seed bypass): PlanAdapter-shaped synthetic hold (floating +
//! fullscreen reconcile) -> explicit send-to-workspace with native
//! send-route flags (never floating, still fullscreen) -> destination
//! still held -> D5 first exit admits at the destination. Neither source
//! nor target reflows at any leg.

use tiler_protocol::planner_protocol::Planner;

fn hold_entry(window: &str, ws: &str) -> serde_json::Value {
    // PlanAdapter `withInitialFullscreenHold` shape: synthetic floating
    // exception, still fullscreen.
    serde_json::json!({
        "window": window, "output": "out-1", "workspace": ws,
        "rect": {"x": 0, "y": 0, "w": 1200, "h": 800},
        "floating": true, "fullscreen": true, "fit_excluded": true,
    })
}

fn plain_entry(window: &str, ws: &str) -> serde_json::Value {
    serde_json::json!({
        "window": window, "output": "out-1", "workspace": ws,
        "rect": {"x": 0, "y": 0, "w": 100, "h": 80},
    })
}

fn native_fullscreen_entry(window: &str, ws: &str) -> serde_json::Value {
    // Workspace-send entry shape: only sticky maps to floating, so the
    // held mover rides never-floating, still fullscreen, fit-excluded.
    serde_json::json!({
        "window": window, "output": "out-1", "workspace": ws,
        "rect": {"x": 0, "y": 0, "w": 1200, "h": 800},
        "fullscreen": true, "fit_excluded": true,
    })
}

fn exited_entry(window: &str, ws: &str) -> serde_json::Value {
    serde_json::json!({
        "window": window, "output": "out-1", "workspace": ws,
        "rect": {"x": 0, "y": 0, "w": 100, "h": 80},
    })
}

fn hold_entry_out(window: &str, output: &str, ws: &str) -> serde_json::Value {
    serde_json::json!({
        "window": window, "output": output, "workspace": ws,
        "rect": {"x": 0, "y": 0, "w": 1200, "h": 800},
        "floating": true, "fullscreen": true, "fit_excluded": true,
    })
}

fn plain_entry_out(window: &str, output: &str, ws: &str) -> serde_json::Value {
    serde_json::json!({
        "window": window, "output": output, "workspace": ws,
        "rect": {"x": 0, "y": 0, "w": 100, "h": 80},
    })
}

fn reconcile(
    correlation: &str,
    workspace: &str,
    focused: &str,
    windows: Vec<serde_json::Value>,
) -> String {
    serde_json::json!({
        "v": 1, "correlation_id": correlation, "owner": "owner-1", "generation": "gen-1",
        "revision": 0, "fingerprint": 7,
        "domain": {"output": "out-1", "workspace": workspace,
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800}, "gap": 0, "outer_gap": 0},
        "focused_window": focused, "windows": windows, "command": {"op": "reconcile"},
    })
    .to_string()
}

fn send(
    correlation: &str,
    focused: &str,
    source: Vec<serde_json::Value>,
    target: Vec<serde_json::Value>,
    follow: bool,
) -> String {
    serde_json::json!({
        "v": 1, "correlation_id": correlation, "owner": "owner-1", "generation": "gen-1",
        "revision": 0, "fingerprint": 7,
        "domain": {"output": "out-1", "workspace": "ws-1",
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800}, "gap": 0, "outer_gap": 0},
        "target_domain": {"output": "out-1", "workspace": "ws-2",
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800}, "gap": 0, "outer_gap": 0},
        "focused_window": focused,
        "windows": source,
        "target_windows": target,
        "command": {"op": "send-to-workspace", "window": focused,
            "target_output": "out-1", "target_workspace": "ws-2", "follow": follow},
    })
    .to_string()
}

fn reply(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("reply is JSON")
}

fn geometry_windows(reply: &serde_json::Value) -> Vec<(String, String)> {
    reply["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|g| {
            (
                g["window"].as_str().unwrap_or("").to_owned(),
                g["workspace"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

/// One Planner, full production sequence with follow.
#[test]
fn slotless_follow_carries_hold_without_reflow_then_exits_at_destination() {
    let mut planner = Planner::new();
    // 1. Source hold: survivor plus synthetic held mover. The mover takes
    // no allocation and the survivor keeps the full domain.
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-hold-1",
        "ws-1",
        "win-s",
        vec![plain_entry("win-s", "ws-1"), hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    let mut members = geometry_windows(&hold);
    members.sort();
    assert_eq!(
        members,
        vec![("win-s".to_owned(), "ws-1".to_owned())],
        "{hold}"
    );
    // 2. Explicit follow send of the held mover with native flags.
    let sent = reply(&planner.evaluate(&send(
        "slotless-send-1",
        "win-f",
        vec![
            plain_entry("win-s", "ws-1"),
            native_fullscreen_entry("win-f", "ws-1"),
        ],
        vec![plain_entry("win-t", "ws-2")],
        true,
    )));
    assert_eq!(sent["outcome"], "planned", "{sent}");
    assert_eq!(sent["operation"]["op"], "move-deferred", "{sent}");
    assert_eq!(sent["operation"]["window"], "win-f", "{sent}");
    assert!(
        sent["operation"].get("leaf").is_none(),
        "no fabricated leaf: {sent}"
    );
    assert_eq!(sent["operation"]["source_workspace"], "ws-1", "{sent}");
    assert_eq!(sent["operation"]["target_workspace"], "ws-2", "{sent}");
    assert_eq!(sent["operation"]["follow"], true, "{sent}");
    // Follow books the target MRU tile (the slotless mover has no leaf).
    assert_eq!(sent["desired_focus"]["domain_workspace"], "ws-2", "{sent}");
    let focus_leaf = sent["desired_focus"]["leaf"].as_str().expect("focus leaf");
    let focus_window = sent["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .find(|g| g["leaf"] == focus_leaf && g["workspace"] == "ws-2")
        .and_then(|g| g["window"].as_str())
        .expect("target focus resolves")
        .to_owned();
    assert_eq!(
        focus_window, "win-t",
        "follow falls back to the target MRU: {sent}"
    );
    // Tiled allocations exactly retained, mover takes none.
    let mut members = geometry_windows(&sent);
    members.sort();
    assert_eq!(
        members,
        vec![
            ("win-s".to_owned(), "ws-1".to_owned()),
            ("win-t".to_owned(), "ws-2".to_owned()),
        ],
        "{sent}"
    );
    for g in sent["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        assert_eq!(
            g["rect"],
            serde_json::json!({"x": 0, "y": 0, "w": 1200, "h": 800}),
            "neither source nor target reflows: {sent}"
        );
    }
    // 3. Destination still holds: synthetic hold at ws-2 admits nothing.
    let dest_hold = reply(&planner.evaluate(&reconcile(
        "slotless-hold-2",
        "ws-2",
        "win-t",
        vec![plain_entry("win-t", "ws-2"), hold_entry("win-f", "ws-2")],
    )));
    assert_eq!(dest_hold["outcome"], "planned", "{dest_hold}");
    let mut members = geometry_windows(&dest_hold);
    members.sort();
    assert_eq!(
        members,
        vec![("win-t".to_owned(), "ws-2".to_owned())],
        "{dest_hold}"
    );
    // Source converges clean without the mover and without reflow.
    let source_clean = reply(&planner.evaluate(&reconcile(
        "slotless-hold-3",
        "ws-1",
        "win-s",
        vec![plain_entry("win-s", "ws-1")],
    )));
    assert_eq!(source_clean["outcome"], "planned", "{source_clean}");
    let mut members = geometry_windows(&source_clean);
    members.sort();
    assert_eq!(
        members,
        vec![("win-s".to_owned(), "ws-1".to_owned())],
        "{source_clean}"
    );
    // 4. D5 first exit admits at the destination, never the source.
    let exit = reply(&planner.evaluate(&reconcile(
        "slotless-exit-1",
        "ws-2",
        "win-t",
        vec![plain_entry("win-t", "ws-2"), exited_entry("win-f", "ws-2")],
    )));
    assert_eq!(exit["outcome"], "planned", "{exit}");
    let mut members = geometry_windows(&exit);
    members.sort();
    assert_eq!(
        members,
        vec![
            ("win-f".to_owned(), "ws-2".to_owned()),
            ("win-t".to_owned(), "ws-2".to_owned()),
        ],
        "first exit admits at the destination: {exit}"
    );
}

/// Stay variant: source MRU survivor keeps focus, mover still carries.
#[test]
fn slotless_stay_keeps_source_mru_without_reflow() {
    let mut planner = Planner::new();
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-stay-hold-1",
        "ws-1",
        "win-s",
        vec![plain_entry("win-s", "ws-1"), hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    let sent = reply(&planner.evaluate(&send(
        "slotless-stay-send-1",
        "win-f",
        vec![
            plain_entry("win-s", "ws-1"),
            native_fullscreen_entry("win-f", "ws-1"),
        ],
        vec![plain_entry("win-t", "ws-2")],
        false,
    )));
    assert_eq!(sent["outcome"], "planned", "{sent}");
    assert_eq!(sent["operation"]["op"], "move-deferred", "{sent}");
    assert_eq!(sent["operation"]["follow"], false, "{sent}");
    assert_eq!(sent["desired_focus"]["domain_workspace"], "ws-1", "{sent}");
    let focus_leaf = sent["desired_focus"]["leaf"].as_str().expect("focus leaf");
    let focus_window = sent["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .find(|g| g["leaf"] == focus_leaf && g["workspace"] == "ws-1")
        .and_then(|g| g["window"].as_str())
        .expect("source focus resolves")
        .to_owned();
    assert_eq!(
        focus_window, "win-s",
        "stay falls back to the source MRU: {sent}"
    );
    let mut members = geometry_windows(&sent);
    members.sort();
    assert_eq!(
        members,
        vec![
            ("win-s".to_owned(), "ws-1".to_owned()),
            ("win-t".to_owned(), "ws-2".to_owned()),
        ],
        "{sent}"
    );
}

/// Flag race: a mover that already exited before dispatch refuses.
#[test]
fn slotless_send_after_exit_refuses_without_transfer() {
    let mut planner = Planner::new();
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-race-hold-1",
        "ws-1",
        "win-s",
        vec![plain_entry("win-s", "ws-1"), hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    // The mover exited fullscreen before the send dispatched: no hold, no
    // carry. The exited window is a plain complete-observation member, so
    // the send route treats it as an ordinary (now tiled-admitting)
    // observation, never as a deferred mover.
    let raced = reply(&planner.evaluate(&send(
        "slotless-race-send-1",
        "win-f",
        vec![plain_entry("win-s", "ws-1"), exited_entry("win-f", "ws-1")],
        vec![plain_entry("win-t", "ws-2")],
        true,
    )));
    assert_ne!(
        raced["operation"]["op"],
        serde_json::Value::String("move-deferred".to_owned()),
        "exited mover never carries deferred: {raced}"
    );
}

fn reconcile_out(
    correlation: &str,
    output: &str,
    workspace: &str,
    focused: &str,
    windows: Vec<serde_json::Value>,
) -> String {
    serde_json::json!({
        "v": 1, "correlation_id": correlation, "owner": "owner-1", "generation": "gen-1",
        "revision": 0, "fingerprint": 7,
        "domain": {"output": output, "workspace": workspace,
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800}, "gap": 0, "outer_gap": 0},
        "focused_window": focused, "windows": windows, "command": {"op": "reconcile"},
    })
    .to_string()
}

fn send_output(
    correlation: &str,
    focused: &str,
    source: Vec<serde_json::Value>,
    target: Vec<serde_json::Value>,
    follow: bool,
) -> String {
    serde_json::json!({
        "v": 1, "correlation_id": correlation, "owner": "owner-1", "generation": "gen-1",
        "revision": 0, "fingerprint": 7,
        "domain": {"output": "out-1", "workspace": "ws-1",
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800}, "gap": 0, "outer_gap": 0},
        "target_domain": {"output": "out-2", "workspace": "ws-1",
            "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800}, "gap": 0, "outer_gap": 0},
        "focused_window": focused,
        "windows": source,
        "target_windows": target,
        "command": {"op": "send-to-output", "window": focused,
            "target_output": "out-2", "target_workspace": "ws-1", "follow": follow},
    })
    .to_string()
}

/// P1: sole slotless source sent to an empty target emits legitimate empty
/// geometry with null focus, moves fullscreen still slotless, and later
/// exits at the destination.
#[test]
fn slotless_empty_target_carries_without_geometry_then_exits() {
    let mut planner = Planner::new();
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-empty-hold-1",
        "ws-1",
        "win-f",
        vec![hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    assert_eq!(
        hold["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "held mover alone takes no allocation: {hold}"
    );
    let sent = reply(&planner.evaluate(&send(
        "slotless-empty-send-1",
        "win-f",
        vec![native_fullscreen_entry("win-f", "ws-1")],
        Vec::<serde_json::Value>::new(),
        true,
    )));
    assert_eq!(sent["outcome"], "planned", "{sent}");
    assert_eq!(sent["operation"]["op"], "move-deferred", "{sent}");
    assert!(
        sent["operation"].get("leaf").is_none(),
        "no fabricated leaf: {sent}"
    );
    assert_eq!(sent["operation"]["target_workspace"], "ws-2", "{sent}");
    assert_eq!(
        sent["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "no tiled members either side, no allocations: {sent}"
    );
    assert_eq!(sent["desired_focus"], serde_json::Value::Null, "{sent}");
    // Destination still holds with no admission.
    let dest_hold = reply(&planner.evaluate(&reconcile(
        "slotless-empty-hold-2",
        "ws-2",
        "win-f",
        vec![hold_entry("win-f", "ws-2")],
    )));
    assert_eq!(dest_hold["outcome"], "planned", "{dest_hold}");
    assert_eq!(
        dest_hold["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "destination hold admits nothing: {dest_hold}"
    );
    // D5 first exit admits at the destination.
    let exit = reply(&planner.evaluate(&reconcile(
        "slotless-empty-exit-1",
        "ws-2",
        "win-f",
        vec![exited_entry("win-f", "ws-2")],
    )));
    assert_eq!(exit["outcome"], "planned", "{exit}");
    let mut members = geometry_windows(&exit);
    members.sort();
    assert_eq!(
        members,
        vec![("win-f".to_owned(), "ws-2".to_owned())],
        "{exit}"
    );
    // The emptied source converges clean with no admission.
    let source_clean = reply(&planner.evaluate(&reconcile(
        "slotless-empty-hold-3",
        "ws-1",
        "",
        Vec::<serde_json::Value>::new(),
    )));
    assert_eq!(source_clean["outcome"], "planned", "{source_clean}");
    assert_eq!(
        source_clean["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "source stays empty with no reflow: {source_clean}"
    );
}

/// P1 stay on the empty-target path: null focus, same deferred carry.
#[test]
fn slotless_empty_target_stay_carries_with_null_focus() {
    let mut planner = Planner::new();
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-empty-stay-hold-1",
        "ws-1",
        "win-f",
        vec![hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    let sent = reply(&planner.evaluate(&send(
        "slotless-empty-stay-send-1",
        "win-f",
        vec![native_fullscreen_entry("win-f", "ws-1")],
        Vec::<serde_json::Value>::new(),
        false,
    )));
    assert_eq!(sent["outcome"], "planned", "{sent}");
    assert_eq!(sent["operation"]["op"], "move-deferred", "{sent}");
    assert_eq!(sent["operation"]["follow"], false, "{sent}");
    assert_eq!(sent["desired_focus"], serde_json::Value::Null, "{sent}");
    assert_eq!(
        sent["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "{sent}"
    );
}

/// Deferred cross-output follow: held mover rides `send-to-output` with
/// cross-output identities and null focus on the member-less target.
#[test]
fn slotless_cross_output_follow_carries_without_geometry() {
    let mut planner = Planner::new();
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-x-hold-1",
        "ws-1",
        "win-f",
        vec![hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    let sent = reply(&planner.evaluate(&send_output(
        "slotless-x-send-1",
        "win-f",
        vec![native_fullscreen_entry("win-f", "ws-1")],
        Vec::<serde_json::Value>::new(),
        true,
    )));
    assert_eq!(sent["outcome"], "planned", "{sent}");
    assert_eq!(sent["kind"], "send-to-output", "{sent}");
    assert_eq!(sent["operation"]["op"], "move-deferred", "{sent}");
    assert!(
        sent["operation"].get("leaf").is_none(),
        "no fabricated leaf: {sent}"
    );
    assert_eq!(sent["operation"]["source_output"], "out-1", "{sent}");
    assert_eq!(sent["operation"]["target_output"], "out-2", "{sent}");
    assert_eq!(
        sent["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "{sent}"
    );
    assert_eq!(sent["desired_focus"], serde_json::Value::Null, "{sent}");
    // Destination holds on the other output with no admission.
    let dest_hold = reply(&planner.evaluate(&reconcile_out(
        "slotless-x-hold-2",
        "out-2",
        "ws-1",
        "win-f",
        vec![hold_entry_out("win-f", "out-2", "ws-1")],
    )));
    assert_eq!(dest_hold["outcome"], "planned", "{dest_hold}");
    assert_eq!(
        dest_hold["desired_geometry"].as_array().map(Vec::len),
        Some(0),
        "cross-output destination hold admits nothing: {dest_hold}"
    );
    // D5 first exit admits at the cross-output destination.
    let exit = reply(&planner.evaluate(&reconcile_out(
        "slotless-x-exit-1",
        "out-2",
        "ws-1",
        "win-f",
        vec![plain_entry_out("win-f", "out-2", "ws-1")],
    )));
    assert_eq!(exit["outcome"], "planned", "{exit}");
    let exit_members: Vec<(String, String, String)> = exit["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|g| {
            (
                g["window"].as_str().unwrap_or("").to_owned(),
                g["output"].as_str().unwrap_or("").to_owned(),
                g["workspace"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect();
    assert_eq!(
        exit_members,
        vec![("win-f".to_owned(), "out-2".to_owned(), "ws-1".to_owned())],
        "first exit admits at the cross-output destination: {exit}"
    );
}

/// Deferred cross-output stay with a source survivor: source MRU keeps
/// focus while the mover still carries across outputs.
#[test]
fn slotless_cross_output_stay_keeps_source_mru() {
    let mut planner = Planner::new();
    let hold = reply(&planner.evaluate(&reconcile(
        "slotless-x-stay-hold-1",
        "ws-1",
        "win-s",
        vec![plain_entry("win-s", "ws-1"), hold_entry("win-f", "ws-1")],
    )));
    assert_eq!(hold["outcome"], "planned", "{hold}");
    let sent = reply(&planner.evaluate(&send_output(
        "slotless-x-stay-send-1",
        "win-f",
        vec![
            plain_entry("win-s", "ws-1"),
            native_fullscreen_entry("win-f", "ws-1"),
        ],
        vec![plain_entry_out("win-t", "out-2", "ws-1")],
        false,
    )));
    assert_eq!(sent["outcome"], "planned", "{sent}");
    assert_eq!(sent["operation"]["op"], "move-deferred", "{sent}");
    assert_eq!(sent["operation"]["follow"], false, "{sent}");
    assert_eq!(sent["operation"]["target_output"], "out-2", "{sent}");
    assert_eq!(sent["desired_focus"]["domain_output"], "out-1", "{sent}");
    let focus_leaf = sent["desired_focus"]["leaf"].as_str().expect("focus leaf");
    let focus_window = sent["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .find(|g| g["leaf"] == focus_leaf && g["output"] == "out-1")
        .and_then(|g| g["window"].as_str())
        .expect("source focus resolves")
        .to_owned();
    assert_eq!(
        focus_window, "win-s",
        "stay falls back to the source MRU: {sent}"
    );
    let mut members = geometry_windows(&sent);
    members.sort();
    assert!(
        members.iter().all(|(w, _)| w != "win-f"),
        "mover takes no allocation: {sent}"
    );
}
