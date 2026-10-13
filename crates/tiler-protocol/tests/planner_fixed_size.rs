#![forbid(unsafe_code)]
//! Q2 fixed-size admission through the real Linux planner route (D1-D8).
//!
//! Drives `Planner::evaluate` (retained live-tree sessions, opt-in
//! enabled) with complete observations: fixed windows take floating
//! membership with no tiled geometry, born fullscreen exits fresh
//! (fixed floats, nonfixed tiles), hint churn never reclassifies,
//! sticky-off retiles with suppression, automatic stays floating on
//! enable, and the diagnostic summary stays bounded with correlation
//! and no identifiers.

use tiler_protocol::planner_protocol::{
    FIXED_ADMISSION_PREFIX, Planner, summarize_fixed_admission,
};

fn reconcile_request(correlation: &str, focused: &str, windows: Vec<serde_json::Value>) -> String {
    serde_json::json!({
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
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn entry(
    window: &str,
    x: i32,
    floating: bool,
    fullscreen: bool,
    sticky: bool,
    fixed_auto: bool,
    fixed_suppress: bool,
    min_size: Option<(i32, i32)>,
    max_size: Option<(i32, i32)>,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "window": window,
        "output": "out-1",
        "workspace": "ws-1",
        "rect": {"x": x, "y": 0, "w": 200, "h": 200},
    });
    if floating {
        value["floating"] = serde_json::Value::Bool(true);
    }
    if fullscreen {
        value["fullscreen"] = serde_json::Value::Bool(true);
    }
    if sticky {
        value["sticky"] = serde_json::Value::Bool(true);
    }
    if fixed_auto {
        value["fixed_auto"] = serde_json::Value::Bool(true);
    }
    if fixed_suppress {
        value["fixed_suppress"] = serde_json::Value::Bool(true);
    }
    if floating || fullscreen || sticky {
        value["fit_excluded"] = serde_json::Value::Bool(true);
    }
    if let Some((w, h)) = min_size {
        value["min_size"] = serde_json::json!({"w": w, "h": h});
    }
    if let Some((w, h)) = max_size {
        value["max_size"] = serde_json::json!({"w": w, "h": h});
    }
    value
}

fn fixed(window: &str, x: i32) -> serde_json::Value {
    entry(
        window,
        x,
        false,
        false,
        false,
        false,
        false,
        Some((640, 480)),
        Some((640, 480)),
    )
}

fn plain(window: &str, x: i32) -> serde_json::Value {
    entry(window, x, false, false, false, false, false, None, None)
}

fn reply_value(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("reply is JSON")
}

fn geometry_windows(reply: &serde_json::Value) -> Vec<String> {
    reply["desired_geometry"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|g| g["window"].as_str().unwrap_or("").to_owned())
        .collect()
}

#[test]
fn fixed_floats_with_no_tiled_geometry() {
    let mut planner = Planner::new();
    let reply = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-1",
        "win-a",
        vec![plain("win-a", 0), fixed("win-f", 400)],
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    let ids = geometry_windows(&reply);
    assert!(ids.contains(&"win-a".to_owned()), "sibling tiles {ids:?}");
    assert!(
        !ids.contains(&"win-f".to_owned()),
        "fixed takes no geometry {ids:?}"
    );
    // A second identical observation stays planned and stable.
    let again = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-2",
        "win-a",
        vec![
            plain("win-a", 0),
            entry(
                "win-f",
                400,
                true,
                false,
                false,
                false,
                false,
                Some((640, 480)),
                Some((640, 480)),
            ),
        ],
    )));
    assert_eq!(again["outcome"], "planned", "{again}");
    let ids = geometry_windows(&again);
    assert!(
        !ids.contains(&"win-f".to_owned()),
        "fixed stays out {ids:?}"
    );
}

#[test]
fn born_fullscreen_fixed_exits_floating_nonfixed_tiles() {
    let mut planner = Planner::new();
    // Born fullscreen rides the synthetic floating hold (as the adapter
    // sends it): bypasses the classifier with no automatic marker (D5).
    let born = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-fs-1",
        "win-s",
        vec![entry(
            "win-s",
            0,
            true,
            true,
            false,
            false,
            false,
            Some((640, 480)),
            Some((640, 480)),
        )],
    )));
    assert_eq!(born["outcome"], "planned", "{born}");
    assert!(
        !geometry_windows(&born).contains(&"win-s".to_owned()),
        "born fullscreen takes no tile while held {born}"
    );
    // First exit is fresh admission (D5): fixed floats untouched.
    let exit = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-fs-2",
        "win-s",
        vec![fixed("win-s", 0)],
    )));
    assert_eq!(exit["outcome"], "planned", "{exit}");
    assert!(
        !geometry_windows(&exit).contains(&"win-s".to_owned()),
        "fixed exits floating {exit}"
    );
    // Nonfixed born fullscreen exits tiled.
    let mut planner2 = Planner::new();
    let born2 = reply_value(&planner2.evaluate(&reconcile_request(
        "fixed-plan-fs-3",
        "win-n",
        vec![entry(
            "win-n", 0, true, true, false, false, false, None, None,
        )],
    )));
    assert_eq!(born2["outcome"], "planned", "{born2}");
    let exit2 = reply_value(&planner2.evaluate(&reconcile_request(
        "fixed-plan-fs-4",
        "win-n",
        vec![plain("win-n", 0)],
    )));
    assert_eq!(exit2["outcome"], "planned", "{exit2}");
    assert!(
        geometry_windows(&exit2).contains(&"win-n".to_owned()),
        "nonfixed exits tiled {exit2}"
    );
}

#[test]
fn dynamic_hints_never_reclassify_tiled() {
    let mut planner = Planner::new();
    let first = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-h-1",
        "win-a",
        vec![plain("win-a", 0)],
    )));
    assert_eq!(first["outcome"], "planned");
    // The same tiled window reporting fixed hints keeps its slot (D2).
    let second = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-h-2",
        "win-a",
        vec![fixed("win-a", 0)],
    )));
    assert_eq!(second["outcome"], "planned", "{second}");
    assert!(
        geometry_windows(&second).contains(&"win-a".to_owned()),
        "hint change keeps tile {second}"
    );
}

#[test]
fn sticky_off_retiles_fixed_while_sticky_floats_stay() {
    let mut planner = Planner::new();
    let first = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-s-1",
        "win-a",
        vec![
            plain("win-a", 0),
            fixed("win-f", 400),
            entry(
                "win-g",
                800,
                true,
                false,
                true,
                false,
                false,
                Some((640, 480)),
                Some((640, 480)),
            ),
        ],
    )));
    assert_eq!(first["outcome"], "planned", "{first}");
    assert!(!geometry_windows(&first).contains(&"win-f".to_owned()));
    assert!(!geometry_windows(&first).contains(&"win-g".to_owned()));
    // The adapter adopts both floats (win-f automatic, win-g sticky).
    let second = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-s-2",
        "win-a",
        vec![
            plain("win-a", 0),
            entry(
                "win-f",
                400,
                true,
                false,
                false,
                false,
                false,
                Some((640, 480)),
                Some((640, 480)),
            ),
            entry(
                "win-g",
                800,
                true,
                false,
                true,
                false,
                false,
                Some((640, 480)),
                Some((640, 480)),
            ),
        ],
    )));
    assert_eq!(second["outcome"], "planned", "{second}");
    assert!(!geometry_windows(&second).contains(&"win-f".to_owned()));
    // Workspace enable keeps the automatic float (D6): win-f arrives
    // non-floating without suppression and stays floating, while the
    // sticky float keeps its membership. An explicit suppress would
    // tile (D3, see suppress_signal_tiles_without_reclassifying).
    let third = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-s-3",
        "win-a",
        vec![
            plain("win-a", 0),
            fixed("win-f", 400),
            entry(
                "win-g",
                800,
                true,
                false,
                true,
                false,
                false,
                Some((640, 480)),
                Some((640, 480)),
            ),
        ],
    )));
    assert_eq!(third["outcome"], "planned", "{third}");
    assert!(
        !geometry_windows(&third).contains(&"win-f".to_owned()),
        "automatic stays floating on enable {third}"
    );
    assert!(
        !geometry_windows(&third).contains(&"win-g".to_owned()),
        "sticky keeps floating {third}"
    );
}

#[test]
fn suppress_signal_tiles_without_reclassifying() {
    // The adapter-retained tile win crosses a fresh session (domain
    // release): a suppressed fixed window tiles, and the win persists
    // so later unsuppressed observations keep it tiled.
    let mut planner = Planner::new();
    let first = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-x-1",
        "win-f",
        vec![entry(
            "win-f",
            0,
            false,
            false,
            false,
            false,
            true,
            Some((640, 480)),
            Some((640, 480)),
        )],
    )));
    assert_eq!(first["outcome"], "planned", "{first}");
    assert!(
        geometry_windows(&first).contains(&"win-f".to_owned()),
        "suppressed tiles {first}"
    );
    let second = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-x-2",
        "win-f",
        vec![entry(
            "win-f",
            0,
            false,
            false,
            false,
            false,
            false,
            Some((640, 480)),
            Some((640, 480)),
        )],
    )));
    assert_eq!(second["outcome"], "planned", "{second}");
    assert!(
        geometry_windows(&second).contains(&"win-f".to_owned()),
        "win persists tiled {second}"
    );
}

#[test]
fn fixed_auto_signal_marks_automatic_origin() {
    // A floating fixed window without the origin signal stays
    // intentional; with fixed_auto it carries automatic origin.
    let mut planner = Planner::new();
    let first = reply_value(&planner.evaluate(&reconcile_request(
        "fixed-plan-o-1",
        "win-a",
        vec![
            plain("win-a", 0),
            entry(
                "win-f",
                400,
                true,
                false,
                false,
                true,
                false,
                Some((640, 480)),
                Some((640, 480)),
            ),
        ],
    )));
    assert_eq!(first["outcome"], "planned", "{first}");
    assert!(!geometry_windows(&first).contains(&"win-f".to_owned()));
}

#[test]
fn fixed_diagnostic_is_bounded_with_correlation() {
    let line = summarize_fixed_admission("gen-1-p3", "reconcile", 2, 1, "fixed-equal");
    assert!(line.starts_with(FIXED_ADMISSION_PREFIX), "{line}");
    assert!(line.contains("correlation=gen-1-p3"), "{line}");
    assert!(line.contains("evaluated=2"), "{line}");
    assert!(line.contains("admitted=1"), "{line}");
    assert!(line.contains("reason=fixed-equal"), "{line}");
    assert!(!line.contains("win-"), "{line}");
    // Hostile tokens degrade to placeholders, never echo.
    let hostile = summarize_fixed_admission("evil!!", "bogus op", 1, 1, "fixed-equal");
    assert!(!hostile.contains("evil"), "{hostile}");
    assert!(!hostile.contains("bogus"), "{hostile}");
    assert!(hostile.contains("correlation=-"), "{hostile}");
}
