#![forbid(unsafe_code)]
//! Production DescribePlan directional cross-output integration (active route).
//!
//! `kwin/src/entry.ts -> startPlanAdapterEntry -> PlanAdapter.requestFocus /
//! requestMove -> DescribePlan -> src/planner_protocol.rs` with a bounded
//! two-domain `domains` payload: source plus the horizontally reciprocal
//! adjacent output's current logical workspace (distinct workspace ids
//! allowed). Asserts S20/S22 crossing, focus last-focus selection with no
//! layout writes, exact membership+geometry/focus shape, gap/inset coverage,
//! and stale/ambiguous/owner/visibility refusals. Up/Down never cross.

use std::collections::BTreeSet;

use tiler_protocol::planner_protocol::Planner as CorePlanner;

/// Test harness which establishes canonical per-domain state through a
/// fresh complete reconcile on each domain before exercising a directional
/// request. Production directional paths never perform this spatial
/// construction.
struct Planner {
    inner: CorePlanner,
    binding: Option<(String, String)>,
    seeded: BTreeSet<(String, String)>,
}

impl Planner {
    fn new() -> Self {
        Self {
            inner: CorePlanner::new(),
            binding: None,
            seeded: BTreeSet::new(),
        }
    }

    fn evaluate(&mut self, request: &str) -> String {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(request) else {
            return self.inner.evaluate(request);
        };
        self.seed_directional_components(&value);
        self.inner.evaluate(request)
    }

    fn seed_directional_components(&mut self, value: &serde_json::Value) {
        let Some(owner) = value.get("owner").and_then(serde_json::Value::as_str) else {
            return;
        };
        let Some(generation) = value.get("generation").and_then(serde_json::Value::as_str) else {
            return;
        };
        let binding = (owner.to_owned(), generation.to_owned());
        if self.binding.as_ref() != Some(&binding) {
            self.binding = Some(binding.clone());
            self.seeded.clear();
        }
        let Some(command) = value.get("command").and_then(serde_json::Value::as_object) else {
            return;
        };
        if !matches!(
            command.get("op").and_then(serde_json::Value::as_str),
            Some("focus" | "move")
        ) {
            return;
        }
        let Some(domains) = value.get("domains").and_then(serde_json::Value::as_array) else {
            return;
        };
        if domains.len() != 2 {
            return;
        }
        let Some(windows) = value.get("windows").and_then(serde_json::Value::as_array) else {
            return;
        };
        let focused = value
            .get("focused_window")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        for (index, domain) in domains.iter().enumerate() {
            let Some(output) = domain.get("output").and_then(serde_json::Value::as_str) else {
                return;
            };
            let Some(workspace) = domain.get("workspace").and_then(serde_json::Value::as_str)
            else {
                return;
            };
            let key = (output.to_owned(), workspace.to_owned());
            if self.seeded.contains(&key) {
                continue;
            }
            let members: Vec<serde_json::Value> = windows
                .iter()
                .filter(|window| {
                    window.get("output").and_then(serde_json::Value::as_str) == Some(output)
                        && window.get("workspace").and_then(serde_json::Value::as_str)
                            == Some(workspace)
                })
                .cloned()
                .collect();
            let Some(seed_focus) = members
                .iter()
                .find(|window| {
                    window.get("window").and_then(serde_json::Value::as_str) == Some(focused)
                })
                .or_else(|| members.last())
                .and_then(|window| window.get("window").and_then(serde_json::Value::as_str))
            else {
                continue;
            };
            let seed = serde_json::json!({
                "v": 1,
                "correlation_id": format!("canonical-seed-{index}"),
                "owner": binding.0,
                "generation": binding.1,
                "revision": 0,
                "fingerprint": 7001,
                "domain": {
                    "output": output,
                    "workspace": workspace,
                    "bounds": domain["bounds"],
                    "gap": domain["gap"],
                    "outer_gap": domain["outer_gap"],
                },
                "focused_window": seed_focus,
                "windows": members,
                "command": {
                    "op": "reconcile",
                },
            });
            let reply: serde_json::Value =
                serde_json::from_str(&self.inner.evaluate(&seed.to_string())).expect("seed reply");
            if reply.get("outcome").and_then(serde_json::Value::as_str) == Some("planned") {
                self.seeded.insert(key);
            }
        }
    }
}

fn source_domain() -> serde_json::Value {
    serde_json::json!({
        "output": "out-2",
        "workspace": "ws-b",
        "bounds": {"x": 800, "y": 0, "w": 800, "h": 600},
        "gap": 4,
        "outer_gap": 8,
    })
}

fn domains_payload() -> serde_json::Value {
    serde_json::json!([
        {
            "output": "out-2",
            "workspace": "ws-b",
            "bounds": {"x": 800, "y": 0, "w": 800, "h": 600},
            "gap": 4,
            "outer_gap": 8,
            "adjacent": {"left": "out-1"},
        },
        {
            "output": "out-1",
            "workspace": "ws-a",
            "bounds": {"x": 0, "y": 0, "w": 800, "h": 600},
            "gap": 4,
            "outer_gap": 8,
            "adjacent": {"right": "out-2"},
        },
    ])
}

// Left-hand source variant for move tests: the focused right-edge window
// moves right into the adjacent target. The reconcile seed keeps the left
// window first and the focused right window last, yielding H[left,
// focused-right] with the mover at the outward right edge (S20 mirror).
fn left_source_domain() -> serde_json::Value {
    serde_json::json!({
        "output": "out-1",
        "workspace": "ws-a",
        "bounds": {"x": 0, "y": 0, "w": 800, "h": 600},
        "gap": 4,
        "outer_gap": 8,
    })
}

fn left_domains_payload() -> serde_json::Value {
    serde_json::json!([
        {
            "output": "out-1",
            "workspace": "ws-a",
            "bounds": {"x": 0, "y": 0, "w": 800, "h": 600},
            "gap": 4,
            "outer_gap": 8,
            "adjacent": {"right": "out-2"},
        },
        {
            "output": "out-2",
            "workspace": "ws-b",
            "bounds": {"x": 800, "y": 0, "w": 800, "h": 600},
            "gap": 4,
            "outer_gap": 8,
            "adjacent": {"left": "out-1"},
        },
    ])
}

fn win(
    window: &str,
    output: &str,
    workspace: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> serde_json::Value {
    serde_json::json!({
        "window": window,
        "output": output,
        "workspace": workspace,
        "rect": {"x": x, "y": y, "w": w, "h": h},
    })
}

fn request(
    correlation: &str,
    focused: &str,
    windows: Vec<serde_json::Value>,
    command: serde_json::Value,
    with_domains: bool,
) -> String {
    request_with_domain(
        correlation,
        focused,
        windows,
        command,
        source_domain(),
        with_domains.then(domains_payload),
    )
}

/// Test mirror of the canonical directional fingerprint (FNV-1a 32-bit over
/// ordered domain primitives, focused id, and id-sorted windows). Must stay
/// byte-identical to `planner_protocol` derivation and the adapter's
/// `planDirectionalFingerprint`; any drift fails `fingerprint-mismatch`.
fn directional_fp(
    domains: &serde_json::Value,
    focused: &str,
    windows: &[serde_json::Value],
) -> u64 {
    fn feed(h: &mut u32, s: &str) {
        for b in s.bytes() {
            *h ^= u32::from(b);
            *h = h.wrapping_mul(16777619);
        }
    }
    fn sep(h: &mut u32, b: u8) {
        *h ^= u32::from(b);
        *h = h.wrapping_mul(16777619);
    }
    let num = |v: &serde_json::Value| v.as_i64().expect("int").to_string();
    let flag = |v: &serde_json::Value, key: &str| {
        if v.get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            "1"
        } else {
            "0"
        }
    };
    let mut h: u32 = 2166136261;
    for (i, d) in domains.as_array().expect("domains").iter().enumerate() {
        if i > 0 {
            sep(&mut h, 0x1e);
        }
        feed(&mut h, d["output"].as_str().expect("output"));
        sep(&mut h, 0x1f);
        feed(&mut h, d["workspace"].as_str().expect("workspace"));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&d["bounds"]["x"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&d["bounds"]["y"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&d["bounds"]["w"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&d["bounds"]["h"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&d["gap"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&d["outer_gap"]));
        sep(&mut h, 0x1f);
        feed(&mut h, "left");
        sep(&mut h, 0x1f);
        feed(
            &mut h,
            d["adjacent"]
                .get("left")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        sep(&mut h, 0x1f);
        feed(&mut h, "right");
        sep(&mut h, 0x1f);
        feed(
            &mut h,
            d["adjacent"]
                .get("right")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        sep(&mut h, 0x1f);
        feed(&mut h, "up");
        sep(&mut h, 0x1f);
        feed(
            &mut h,
            d["adjacent"]
                .get("up")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        sep(&mut h, 0x1f);
        feed(&mut h, "down");
        sep(&mut h, 0x1f);
        feed(
            &mut h,
            d["adjacent"]
                .get("down")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
    }
    sep(&mut h, 0x1f);
    feed(&mut h, focused);
    let mut ordered: Vec<&serde_json::Value> = windows.iter().collect();
    ordered.sort_by(|a, b| a["window"].as_str().cmp(&b["window"].as_str()));
    for w in ordered {
        sep(&mut h, 0x1f);
        feed(&mut h, w["window"].as_str().expect("window"));
        sep(&mut h, 0x1f);
        feed(&mut h, w["output"].as_str().expect("output"));
        sep(&mut h, 0x1f);
        feed(&mut h, w["workspace"].as_str().expect("workspace"));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&w["rect"]["x"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&w["rect"]["y"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&w["rect"]["w"]));
        sep(&mut h, 0x1f);
        feed(&mut h, &num(&w["rect"]["h"]));
        sep(&mut h, 0x1f);
        feed(&mut h, flag(w, "floating"));
        sep(&mut h, 0x1f);
        feed(&mut h, flag(w, "fit_excluded"));
    }
    u64::from(h)
}

fn request_with_domain(
    correlation: &str,
    focused: &str,
    windows: Vec<serde_json::Value>,
    command: serde_json::Value,
    domain: serde_json::Value,
    domains: Option<serde_json::Value>,
) -> String {
    // Directional requests bind the full two-domain evidence in the
    // fingerprint (Rust re-derives it); legacy requests keep a constant.
    let fingerprint = match &domains {
        Some(entries) => directional_fp(entries, focused, &windows),
        None => 7001,
    };
    let mut value = serde_json::json!({
        "v": 1,
        "correlation_id": correlation,
        "owner": "owner-1",
        "generation": "gen-1",
        "revision": 0,
        "fingerprint": fingerprint,
        "domain": domain,
        "focused_window": focused,
        "windows": windows,
        "command": command,
    });
    if let Some(domains) = domains {
        value["domains"] = domains;
    }
    value.to_string()
}

fn parse(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("reply is JSON")
}

fn focus_cmd(window: &str, direction: &str) -> serde_json::Value {
    serde_json::json!({"op": "focus", "window": window, "direction": direction})
}

fn move_cmd(window: &str, direction: &str) -> serde_json::Value {
    serde_json::json!({"op": "move", "window": window, "direction": direction})
}

fn one_each_windows() -> Vec<serde_json::Value> {
    vec![
        win("win-a", "out-2", "ws-b", 810, 10, 100, 80),
        win("win-x", "out-1", "ws-a", 10, 10, 100, 80),
    ]
}

#[test]
fn directional_fingerprint_golden_vector() {
    // Cross-language pin with the adapter's `planDirectionalFingerprint`
    // test: both derivations must emit this exact value for the same input.
    // Recompute here only by changing the canonical scheme on both sides.
    // Item 5.2 scheme: left/right/up/down adjacency in fixed order (was
    // left/right only); the adapter pin must move in sync.
    let fp = directional_fp(&domains_payload(), "win-a", &one_each_windows());
    assert_eq!(fp, 2130914552, "golden directional fingerprint");
}

#[test]
fn focus_cross_selects_target_with_exact_operation_and_full_geometry() {
    let mut planner = Planner::new();
    let reply = parse(&planner.evaluate(&request(
        "dir-focus-1",
        "win-a",
        one_each_windows(),
        focus_cmd("win-a", "left"),
        true,
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    // Target is the adjacent output's current workspace (ws-a, not ws-b).
    assert_eq!(reply["desired_focus"]["domain_output"], "out-1", "{reply}");
    assert_eq!(
        reply["desired_focus"]["domain_workspace"], "ws-a",
        "{reply}"
    );
    // Exact cross operation fields with explicit source binding.
    let op = &reply["operation"];
    assert_eq!(op["op"], "focus", "{reply}");
    assert_eq!(op["domain_output"], "out-1", "{reply}");
    assert_eq!(op["domain_workspace"], "ws-a", "{reply}");
    assert_eq!(op["from_window"], "win-a", "{reply}");
    assert_eq!(op["to_window"], "win-x", "{reply}");
    assert_eq!(op["direction"], "left", "{reply}");
    assert_eq!(op["cross_source_output"], "out-2", "{reply}");
    assert_eq!(op["cross_source_workspace"], "ws-b", "{reply}");
    assert_eq!(op["route"].as_array().map(Vec::len), Some(1), "{reply}");
    // Exact preconditions with the adjacent-output token.
    assert_eq!(
        reply["preconditions"],
        serde_json::json!([
            "focused-leaf-occupied-by-focused-window",
            "target-leaf-occupied",
            "focus-targets-adjacent-output",
            "adapter-must-verify-postconditions",
        ]),
        "{reply}"
    );
    // Desired geometry covers both domains (source + target).
    let geometry = reply["desired_geometry"].as_array().expect("geometry");
    assert_eq!(geometry.len(), 2, "{reply}");
    let mut outputs: Vec<(&str, &str)> = geometry
        .iter()
        .map(|g| {
            (
                g["output"].as_str().expect("output"),
                g["workspace"].as_str().expect("workspace"),
            )
        })
        .collect();
    outputs.sort();
    assert_eq!(
        outputs,
        vec![("out-1", "ws-a"), ("out-2", "ws-b")],
        "{reply}"
    );
    // Gap/inset complete coverage: every rect positive and contained in its
    // inset work area ((808,8,784,584) source, (8,8,784,584) target).
    for entry in geometry {
        let (x, y, w, h) = (
            entry["rect"]["x"].as_i64().expect("x"),
            entry["rect"]["y"].as_i64().expect("y"),
            entry["rect"]["w"].as_i64().expect("w"),
            entry["rect"]["h"].as_i64().expect("h"),
        );
        assert!(w > 0 && h > 0, "{reply}");
        let (bx, by, bw, bh) = if entry["output"] == "out-2" {
            (808, 8, 784, 584)
        } else {
            (8, 8, 784, 584)
        };
        assert!(
            x >= bx && y >= by && x + w <= bx + bw && y + h <= by + bh,
            "rect contained: {reply}"
        );
    }
}

#[test]
fn focus_up_never_crosses_and_stays_local_refusal() {
    let mut planner = Planner::new();
    // A lone root leaf has no local Up target: local Unchanged refuses, and
    // the directional route never crosses vertically.
    let reply = parse(&planner.evaluate(&request(
        "dir-focus-up-1",
        "win-a",
        one_each_windows(),
        focus_cmd("win-a", "up"),
        true,
    )));
    assert_eq!(reply["outcome"], "rejected", "{reply}");
    assert_eq!(reply["kind"], "unchanged", "{reply}");
    assert!(reply.get("operation").is_none(), "{reply}");
}

#[test]
fn move_cross_occupied_plans_r4_with_target_focus() {
    // S20 equivalent with distinct workspaces: source H[win-a, win-b] with
    // win-b focused at the right root edge moves right into occupied out-2.
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-1", "ws-a", 10, 10, 100, 80),
        win("win-b", "out-1", "ws-a", 400, 10, 100, 80),
        win("win-x", "out-2", "ws-b", 810, 10, 100, 80),
    ];
    let reply = parse(&planner.evaluate(&request_with_domain(
        "dir-move-1",
        "win-b",
        windows,
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert_eq!(reply["detail"]["rule"], "R4", "{reply}");
    assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
    assert_eq!(
        reply["desired_focus"]["domain_workspace"], "ws-b",
        "{reply}"
    );
    let geometry = reply["desired_geometry"].as_array().expect("geometry");
    assert_eq!(geometry.len(), 3, "{reply}");
}

#[test]
fn move_cross_empty_target_plans_r4() {
    // S22 equivalent: source H[win-a, win-b] with win-b focused moves right
    // into the empty adjacent target; the mover lands on out-2/ws-b.
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-1", "ws-a", 10, 10, 100, 80),
        win("win-b", "out-1", "ws-a", 400, 10, 100, 80),
    ];
    let reply = parse(&planner.evaluate(&request_with_domain(
        "dir-move-empty-1",
        "win-b",
        windows,
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert_eq!(reply["detail"]["rule"], "R4", "{reply}");
    assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
    assert_eq!(
        reply["desired_focus"]["domain_workspace"], "ws-b",
        "{reply}"
    );
    // Source collapses to win-a; the mover lands on the target.
    let geometry = reply["desired_geometry"].as_array().expect("geometry");
    assert_eq!(geometry.len(), 2, "{reply}");
    let mover = geometry
        .iter()
        .find(|g| g["window"] == "win-b")
        .expect("mover geometry");
    assert_eq!(mover["output"], "out-2", "{reply}");
    assert_eq!(mover["workspace"], "ws-b", "{reply}");
}

#[test]
fn move_up_without_vertical_candidate_stays_local() {
    // No up-adjacent output exists in this pair (left/right adjacency only),
    // so the exhausted Up move stays local: no candidate is a local outcome,
    // never a cross. Vertical crossing with a real up-candidate is covered
    // by the stacked unit route (`directional_vertical_pair_crosses_after_exhaustion`).
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-2", "ws-b", 810, 10, 300, 200),
        win("win-b", "out-2", "ws-b", 810, 300, 300, 200),
        win("win-x", "out-1", "ws-a", 10, 10, 100, 80),
    ];
    let reply = parse(&planner.evaluate(&request(
        "dir-move-up-1",
        "win-b",
        windows,
        move_cmd("win-b", "up"),
        true,
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert_ne!(reply["detail"]["rule"], "R4", "{reply}");
    assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
    assert_eq!(
        reply["desired_focus"]["domain_workspace"], "ws-b",
        "{reply}"
    );
}

#[test]
fn directional_refusals_fail_closed() {
    // Ambiguous duplicate output ids.
    {
        let mut planner = Planner::new();
        let mut value: serde_json::Value = serde_json::from_str(&request(
            "dir-ref-amb-1",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        ))
        .expect("valid");
        value["domains"] = serde_json::json!([
            {"output": "out-2", "workspace": "ws-b",
             "bounds": {"x": 800, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {"left": "out-1"}},
            {"output": "out-1", "workspace": "ws-a",
             "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {"right": "out-2"}},
            {"output": "out-1", "workspace": "ws-c",
             "bounds": {"x": 1600, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {"right": "out-2"}},
        ]);
        let reply = parse(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Non-reciprocal adjacency.
    {
        let mut planner = Planner::new();
        let mut value: serde_json::Value = serde_json::from_str(&request(
            "dir-ref-rec-1",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        ))
        .expect("valid");
        value["domains"] = serde_json::json!([
            {"output": "out-2", "workspace": "ws-b",
             "bounds": {"x": 800, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {"left": "out-1"}},
            {"output": "out-1", "workspace": "ws-a",
             "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {}},
        ]);
        let reply = parse(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Vertical adjacency keys never admitted.
    {
        let mut planner = Planner::new();
        let mut value: serde_json::Value = serde_json::from_str(&request(
            "dir-ref-vert-1",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        ))
        .expect("valid");
        value["domains"] = serde_json::json!([
            {"output": "out-2", "workspace": "ws-b",
             "bounds": {"x": 800, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {"up": "out-1"}},
            {"output": "out-1", "workspace": "ws-a",
             "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
             "adjacent": {"down": "out-2"}},
        ]);
        let reply = parse(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Missing target window: partial observation.
    {
        let mut planner = Planner::new();
        let reply = parse(&planner.evaluate(&request(
            "dir-ref-miss-1",
            "win-a",
            vec![win("win-a", "out-2", "ws-b", 810, 10, 100, 80)],
            focus_cmd("win-a", "left"),
            false,
        )));
        // Single-domain legacy path without the target: local focus has no
        // target and refuses unchanged (no cross without domains).
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Owner change across calls re-binds without poisoning: the first call
    // seeds under owner-1, the second under owner-9 plans the same cross
    // focus from a fresh seed (never a stale substitution).
    {
        let mut planner = Planner::new();
        let first = parse(&planner.evaluate(&request(
            "dir-ref-owner-1",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        )));
        assert_eq!(first["outcome"], "planned", "{first}");
        let mut value: serde_json::Value = serde_json::from_str(&request(
            "dir-ref-owner-2",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        ))
        .expect("valid");
        value["owner"] = serde_json::json!("owner-9");
        let reply = parse(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["desired_focus"]["domain_output"], "out-1", "{reply}");
    }
    // Duplicate window ids fail closed.
    {
        let mut planner = Planner::new();
        let mut windows = one_each_windows();
        windows.push(win("win-a", "out-1", "ws-a", 10, 10, 100, 80));
        let reply = parse(&planner.evaluate(&request(
            "dir-ref-dup-1",
            "win-a",
            windows,
            focus_cmd("win-a", "left"),
            true,
        )));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Focused window outside the source domain fails closed.
    {
        let mut planner = Planner::new();
        let reply = parse(&planner.evaluate(&request(
            "dir-ref-foc-1",
            "win-x",
            one_each_windows(),
            focus_cmd("win-x", "left"),
            true,
        )));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Domains on a non-directional op refuse.
    {
        let mut planner = Planner::new();
        let reply = parse(&planner.evaluate(&request(
            "dir-ref-op-1",
            "win-a",
            one_each_windows(),
            serde_json::json!({"op": "resize", "window": "win-a", "direction": "left", "mode": "outwards", "press_index": 0}),
            true,
        )));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
}

#[test]
fn fingerprint_tampering_fails_at_request_validation() {
    // Altered target rect, bounds, gap, or adjacency must fail here at
    // request validation (`fingerprint-mismatch` or shape refusal), not
    // only at reply revalidation downstream.
    let base_windows = one_each_windows();
    let base_domains = domains_payload();
    let tamper_window_rect = {
        let mut windows = base_windows.clone();
        windows[1]["rect"]["x"] = serde_json::json!(20);
        windows
    };
    let tamper_target_bounds = {
        let mut domains = base_domains.clone();
        domains[1]["bounds"]["w"] = serde_json::json!(700);
        domains
    };
    let tamper_gap = {
        let mut domains = base_domains.clone();
        domains[1]["gap"] = serde_json::json!(5);
        domains
    };
    // Reciprocity-breaking adjacency fails as domain-invalid at validation.
    let tamper_adjacency = {
        let mut domains = base_domains.clone();
        domains[1]["adjacent"] = serde_json::json!({});
        domains
    };
    let cases: Vec<(&str, Vec<serde_json::Value>, serde_json::Value, &str)> = vec![
        (
            "rect",
            tamper_window_rect,
            base_domains.clone(),
            "fingerprint-mismatch",
        ),
        (
            "bounds",
            base_windows.clone(),
            tamper_target_bounds,
            "fingerprint-mismatch",
        ),
        (
            "gap",
            base_windows.clone(),
            tamper_gap,
            "fingerprint-mismatch",
        ),
        (
            "adjacency",
            base_windows.clone(),
            tamper_adjacency,
            "domain-invalid",
        ),
    ];
    for (tag, windows, domains, detail) in cases {
        let mut planner = Planner::new();
        // Fingerprint computed over the UNtampered evidence would pass;
        // here the request helper binds the tampered evidence, so a stale
        // pre-tamper fingerprint must also fail. Build both variants.
        let fp_tampered = directional_fp(&domains, "win-a", &windows);
        let mut value: serde_json::Value = serde_json::from_str(&request_with_domain(
            &format!("dir-fp-{tag}-1"),
            "win-a",
            windows.clone(),
            focus_cmd("win-a", "left"),
            source_domain(),
            Some(domains.clone()),
        ))
        .expect("valid");
        // Sanity: helper-bound fingerprint matches the tampered evidence…
        assert_eq!(value["fingerprint"].as_u64().expect("fp"), fp_tampered);
        // …while the pre-tamper fingerprint does not.
        let fp_clean = directional_fp(&base_domains, "win-a", &base_windows);
        assert_ne!(fp_tampered, fp_clean, "{tag}");
        value["fingerprint"] = serde_json::json!(fp_clean);
        let reply = parse(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{tag} {reply}");
        assert_eq!(reply["kind"], "snapshot-invalid", "{tag} {reply}");
        assert_eq!(reply["detail"], detail, "{tag} {reply}");
    }
}

#[test]
fn stale_adjacency_does_not_poison_retained_pair() {
    let mut planner = Planner::new();
    let first = parse(&planner.evaluate(&request(
        "dir-stale-1",
        "win-a",
        one_each_windows(),
        focus_cmd("win-a", "left"),
        true,
    )));
    assert_eq!(first["outcome"], "planned", "{first}");
    // Same windows, changed adjacency (target no longer reciprocal):
    // rejected at validation, retained pair untouched.
    let mut stale: serde_json::Value = serde_json::from_str(&request(
        "dir-stale-2",
        "win-a",
        one_each_windows(),
        focus_cmd("win-a", "left"),
        true,
    ))
    .expect("valid");
    stale["domains"][1]["adjacent"] = serde_json::json!({});
    // Rebind the fingerprint to the stale evidence so the refusal proves
    // adjacency matching, not fingerprint binding.
    stale["fingerprint"] = serde_json::json!(directional_fp(
        &stale["domains"],
        "win-a",
        &one_each_windows(),
    ));
    let refused = parse(&planner.evaluate(&stale.to_string()));
    assert_eq!(refused["outcome"], "rejected", "{refused}");
    // The original evidence still plans: no poisoning, no wedge.
    let revived = parse(&planner.evaluate(&request(
        "dir-stale-3",
        "win-a",
        one_each_windows(),
        focus_cmd("win-a", "left"),
        true,
    )));
    assert_eq!(revived["outcome"], "planned", "{revived}");
    assert_eq!(
        revived["desired_focus"]["domain_output"], "out-1",
        "{revived}"
    );
}

#[test]
fn many_pairs_plan_without_domain_cap() {
    // No retained-domain count cap: ten distinct pairs (20 domains, beyond
    // the old 16-domain bound) all establish canonical components and plan,
    // and the first pair still plans afterwards.
    let mut planner = Planner::new();
    let pair_request = |tag: &str, i: usize| {
        let out_l = format!("out-l{i}");
        let out_r = format!("out-r{i}");
        let ws = format!("ws-{i}");
        let domains = serde_json::json!([
            {"output": out_l, "workspace": ws,
             "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 0, "outer_gap": 0,
             "adjacent": {"right": out_r}},
            {"output": out_r, "workspace": ws,
             "bounds": {"x": 800, "y": 0, "w": 800, "h": 600}, "gap": 0, "outer_gap": 0,
             "adjacent": {"left": out_l}},
        ]);
        let wins = vec![
            win("win-a", &out_l, &ws, 10, 10, 100, 80),
            win("win-x", &out_r, &ws, 810, 10, 100, 80),
        ];
        let domain = serde_json::json!({
            "output": out_l, "workspace": ws,
            "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 0, "outer_gap": 0,
        });
        request_with_domain(
            tag,
            "win-a",
            wins,
            focus_cmd("win-a", "right"),
            domain,
            Some(domains),
        )
    };
    for i in 0..10 {
        let reply = parse(&planner.evaluate(&pair_request(&format!("dir-cap-{i}"), i)));
        assert_eq!(reply["outcome"], "planned", "pair {i}: {reply}");
    }
    // The eleventh pair also plans, then the first pair still plans.
    let extra = parse(&planner.evaluate(&pair_request("dir-cap-10", 10)));
    assert_eq!(extra["outcome"], "planned", "{extra}");
    let first_again = parse(&planner.evaluate(&pair_request("dir-cap-0b", 0)));
    assert_eq!(first_again["outcome"], "planned", "{first_again}");
    assert_eq!(
        first_again["desired_focus"]["domain_output"], "out-r0",
        "{first_again}"
    );
}

#[test]
fn move_cross_carries_exact_r4_operation() {
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-1", "ws-a", 10, 10, 100, 80),
        win("win-b", "out-1", "ws-a", 400, 10, 100, 80),
        win("win-x", "out-2", "ws-b", 810, 10, 100, 80),
    ];
    let reply = parse(&planner.evaluate(&request_with_domain(
        "dir-move-op-1",
        "win-b",
        windows,
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    let op = &reply["operation"];
    assert_eq!(op["op"], "move", "{reply}");
    assert_eq!(op["rule"], "R4", "{reply}");
    assert_eq!(op["capability"], "CrossOutputTransfer", "{reply}");
    assert_eq!(op["direction"], "right", "{reply}");
    assert_eq!(op["window"], "win-b", "{reply}");
    assert_eq!(op["source_output"], "out-1", "{reply}");
    assert_eq!(op["source_workspace"], "ws-a", "{reply}");
    assert_eq!(op["target_output"], "out-2", "{reply}");
    assert_eq!(op["target_workspace"], "ws-b", "{reply}");
    assert_eq!(op["target"], "occupied", "{reply}");
    assert_eq!(
        reply["preconditions"],
        serde_json::json!([
            "focused-leaf-occupied-by-focused-window",
            "source-root-membership-and-adjacent-same-workspace-output",
            "adapter-must-verify-postconditions",
        ]),
        "{reply}"
    );
}

#[test]
fn move_cross_empty_target_operation_marks_empty() {
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-1", "ws-a", 10, 10, 100, 80),
        win("win-b", "out-1", "ws-a", 400, 10, 100, 80),
    ];
    let reply = parse(&planner.evaluate(&request_with_domain(
        "dir-move-op-2",
        "win-b",
        windows,
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert_eq!(reply["operation"]["target"], "empty", "{reply}");
    assert_eq!(reply["operation"]["rule"], "R4", "{reply}");
}

#[test]
fn active_adapter_capability_refuses_r4_without_staging_state() {
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-1", "ws-a", 10, 10, 100, 80),
        win("win-b", "out-1", "ws-a", 400, 10, 100, 80),
        win("win-x", "out-2", "ws-b", 810, 10, 100, 80),
    ];
    let mut disabled: serde_json::Value = serde_json::from_str(&request_with_domain(
        "dir-r4-disabled-1",
        "win-b",
        windows.clone(),
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    ))
    .expect("valid request");
    disabled["command"]["cross_output_transfer"] = serde_json::json!(false);
    let refused = parse(&planner.evaluate(&disabled.to_string()));
    assert_eq!(refused["outcome"], "rejected", "{refused}");
    assert_eq!(refused["kind"], "unsupported-capability", "{refused}");

    // The refusal did not stage a cross move: a capable follow-up starts from
    // the original source/target membership and plans normally.
    let retry = parse(&planner.evaluate(&request_with_domain(
        "dir-r4-disabled-2",
        "win-b",
        windows,
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    )));
    assert_eq!(retry["outcome"], "planned", "{retry}");
    assert_eq!(retry["detail"]["rule"], "R4", "{retry}");
}

#[test]
fn s21_perpendicular_move_stays_local_r1() {
    // S21 equivalent: the source holds a perpendicular group with win-b
    // focused, and `Meta+Left` applies the local perpendicular wrap (R1)
    // instead of crossing. Step one turns the seeded H group into a V group
    // via Up (R1); step two moves Left inside that V group (R1 again).
    // Neither step crosses, carries an operation, or leaks geometry.
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-2", "ws-b", 810, 10, 300, 200),
        win("win-b", "out-2", "ws-b", 1150, 10, 300, 200),
        win("win-x", "out-1", "ws-a", 10, 10, 100, 80),
    ];
    let first = parse(&planner.evaluate(&request(
        "dir-s21-1",
        "win-b",
        windows.clone(),
        move_cmd("win-b", "up"),
        true,
    )));
    assert_eq!(first["outcome"], "planned", "{first}");
    assert_eq!(first["detail"]["rule"], "R1", "{first}");
    assert!(first.get("operation").is_none(), "{first}");
    let reply = parse(&planner.evaluate(&request(
        "dir-s21-2",
        "win-b",
        windows,
        move_cmd("win-b", "left"),
        true,
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert_eq!(reply["detail"]["rule"], "R1", "{reply}");
    assert!(reply.get("operation").is_none(), "{reply}");
    assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
    assert_eq!(
        reply["desired_focus"]["domain_workspace"], "ws-b",
        "{reply}"
    );
    // Geometry stays source-only: no cross-domain leak.
    for entry in reply["desired_geometry"].as_array().expect("geometry") {
        assert_eq!(entry["output"], "out-2", "{reply}");
        assert_eq!(entry["workspace"], "ws-b", "{reply}");
    }
}

#[test]
fn multiwindow_target_preserves_members_and_focuses_mover() {
    // Occupied multiwindow target: all target members survive with distinct
    // positive contained rects (no root collapse), the mover joins the
    // target beside the remembered leaf, the source collapses, focus follows.
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-1", "ws-a", 10, 10, 100, 80),
        win("win-b", "out-1", "ws-a", 400, 10, 100, 80),
        win("win-x", "out-2", "ws-b", 810, 10, 200, 200),
        win("win-y", "out-2", "ws-b", 810, 300, 200, 200),
    ];
    let reply = parse(&planner.evaluate(&request_with_domain(
        "dir-mw-1",
        "win-b",
        windows,
        move_cmd("win-b", "right"),
        left_source_domain(),
        Some(left_domains_payload()),
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert_eq!(reply["detail"]["rule"], "R4", "{reply}");
    let geometry = reply["desired_geometry"].as_array().expect("geometry");
    assert_eq!(geometry.len(), 4, "{reply}");
    // Mover on the target; every target member present exactly once.
    let mut seen: Vec<&str> = geometry
        .iter()
        .map(|g| g["window"].as_str().expect("window"))
        .collect();
    seen.sort();
    assert_eq!(seen, vec!["win-a", "win-b", "win-x", "win-y"], "{reply}");
    let mover = geometry
        .iter()
        .find(|g| g["window"] == "win-b")
        .expect("mover");
    assert_eq!(mover["output"], "out-2", "{reply}");
    assert_eq!(mover["workspace"], "ws-b", "{reply}");
    // Source collapses to win-a alone.
    let source: Vec<&serde_json::Value> =
        geometry.iter().filter(|g| g["output"] == "out-1").collect();
    assert_eq!(source.len(), 1, "{reply}");
    assert_eq!(source[0]["window"], "win-a", "{reply}");
    // Complete per-domain contained geometry with gaps.
    for entry in geometry {
        let (x, y, w, h) = (
            entry["rect"]["x"].as_i64().expect("x"),
            entry["rect"]["y"].as_i64().expect("y"),
            entry["rect"]["w"].as_i64().expect("w"),
            entry["rect"]["h"].as_i64().expect("h"),
        );
        assert!(w > 0 && h > 0, "{reply}");
        let (bx, by, bw, bh) = if entry["output"] == "out-2" {
            (808, 8, 784, 584)
        } else {
            (8, 8, 784, 584)
        };
        assert!(
            x >= bx && y >= by && x + w <= bx + bw && y + h <= by + bh,
            "{reply}"
        );
    }
    assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
}

#[test]
fn s23_cross_and_return_round_trip() {
    // S23 equivalent: cross to the target, then cross back through the
    // reverse ordered pair. Both directions plan against retained sessions
    // with the target's current workspaces.
    let mut planner = Planner::new();
    let out = parse(&planner.evaluate(&request(
        "dir-s23-1",
        "win-a",
        one_each_windows(),
        focus_cmd("win-a", "left"),
        true,
    )));
    assert_eq!(out["outcome"], "planned", "{out}");
    assert_eq!(out["desired_focus"]["domain_output"], "out-1", "{out}");
    // Reverse pair: source out-1/ws-a, target out-2/ws-b, focus win-x right.
    let reverse_domains = serde_json::json!([
        {"output": "out-1", "workspace": "ws-a",
         "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
         "adjacent": {"right": "out-2"}},
        {"output": "out-2", "workspace": "ws-b",
         "bounds": {"x": 800, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
         "adjacent": {"left": "out-1"}},
    ]);
    let reverse_source = serde_json::json!({
        "output": "out-1", "workspace": "ws-a",
        "bounds": {"x": 0, "y": 0, "w": 800, "h": 600}, "gap": 4, "outer_gap": 8,
    });
    let back = parse(&planner.evaluate(&request_with_domain(
        "dir-s23-2",
        "win-x",
        one_each_windows(),
        focus_cmd("win-x", "right"),
        reverse_source,
        Some(reverse_domains),
    )));
    assert_eq!(back["outcome"], "planned", "{back}");
    assert_eq!(back["desired_focus"]["domain_output"], "out-2", "{back}");
    assert_eq!(back["desired_focus"]["domain_workspace"], "ws-b", "{back}");
}

#[test]
fn visibility_and_revision_fences_hold_then_recover() {
    // Unknown workspace: cross-domain mismatch, no wedge.
    {
        let mut planner = Planner::new();
        let mut windows = one_each_windows();
        windows[1]["workspace"] = serde_json::json!("ws-ghost");
        let reply = parse(&planner.evaluate(&request(
            "dir-vis-1",
            "win-a",
            windows,
            focus_cmd("win-a", "left"),
            true,
        )));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        // Fresh valid evidence still plans afterwards.
        let revived = parse(&planner.evaluate(&request(
            "dir-vis-2",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        )));
        assert_eq!(revived["outcome"], "planned", "{revived}");
    }
    // Missing target member: rejected without wedge.
    {
        let mut planner = Planner::new();
        let reply = parse(&planner.evaluate(&request(
            "dir-vis-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-b", 810, 10, 100, 80)],
            focus_cmd("win-a", "left"),
            true,
        )));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
    // Oversize revision: rejected before planning.
    {
        let mut planner = Planner::new();
        let mut value: serde_json::Value = serde_json::from_str(&request(
            "dir-vis-4",
            "win-a",
            one_each_windows(),
            focus_cmd("win-a", "left"),
            true,
        ))
        .expect("valid");
        value["revision"] = serde_json::json!(1_000_001);
        let reply = parse(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
    }
}

#[test]
fn single_domain_legacy_unchanged_without_domains() {
    let mut planner = Planner::new();
    // No `domains` field: legacy single-domain focus plans locally.
    let windows = vec![
        win("win-a", "out-2", "ws-b", 810, 10, 300, 200),
        win("win-b", "out-2", "ws-b", 1150, 10, 300, 200),
    ];
    let reply = parse(&planner.evaluate(&request(
        "dir-legacy-1",
        "win-b",
        windows,
        focus_cmd("win-b", "left"),
        false,
    )));
    assert_eq!(reply["outcome"], "planned", "{reply}");
    assert!(reply.get("operation").is_none(), "{reply}");
    assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
}

#[test]
fn r1_r3_stay_synchronous_without_pending() {
    // Local moves commit synchronously: no ack/verify round trip, no R4
    // operation, and an immediate follow-up plans (no pending-exists).
    let mut planner = Planner::new();
    let windows = vec![
        win("win-a", "out-2", "ws-b", 810, 10, 300, 200),
        win("win-b", "out-2", "ws-b", 1150, 10, 300, 200),
        win("win-x", "out-1", "ws-a", 10, 10, 100, 80),
    ];
    let first = parse(&planner.evaluate(&request(
        "dir-sync-1",
        "win-b",
        windows.clone(),
        move_cmd("win-b", "left"),
        true,
    )));
    assert_eq!(first["outcome"], "planned", "{first}");
    assert_ne!(first["detail"]["rule"], "R4", "{first}");
    assert!(first.get("operation").is_none(), "{first}");
    // Synchronous commit stored the canonical source: a legacy single-domain
    // follow-up on the source members plans immediately (no pending).
    let follow = parse(&planner.evaluate(&request_with_domain(
        "dir-sync-2",
        "win-a",
        vec![
            win("win-a", "out-2", "ws-b", 810, 10, 300, 200),
            win("win-b", "out-2", "ws-b", 1150, 10, 300, 200),
        ],
        focus_cmd("win-a", "left"),
        source_domain(),
        None,
    )));
    assert_eq!(follow["outcome"], "planned", "{follow}");
}

// R4 cancel helper: same two-domain envelope as status (rebased revision,
// rebound fingerprint) with the cancellation command carrying the
// zero-dispatch attestation and the exact dispatch-time pre-observation.
// Revision is always the original request revision (0), never a base learned
// from a stale probe.
