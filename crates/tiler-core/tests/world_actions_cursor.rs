//! Shared world-action acceptance (stage 2c): runs every case in
//! `test-fixtures/world-actions-cursor.json` through the core engine with no
//! duplicated expectations. The fixture owns the scene, the per-case
//! cursor/focus actions, and the expected policy output/anchor; this runner
//! only translates the normalized scene into reports and asserts the shared
//! relationships (homing, anchor shrink, newcomer-plus-anchor union, stable
//! peers, source tiles, target view, settlement). The Windows production
//! `LiveNative` seam consumes the same file; a later live connector scales
//! the normalized scene to host pixels.

use tiler_core::cursor_targeting::{FreshPlacement, SwitchTarget};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{WorldCommand, WorldReport};

fn rect(v: &[i64]) -> Rect {
    Rect {
        x: v[0] as i32,
        y: v[1] as i32,
        w: v[2] as i32,
        h: v[3] as i32,
    }
}

fn load_fixture() -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-actions-cursor.json");
    let raw = std::fs::read_to_string(&path).expect("world-actions-cursor fixture present");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("valid fixture JSON");
    assert_eq!(fixture["version"], serde_json::Value::from(1));
    fixture
}

fn screen(output: &str, x: i32) -> tiler_core::world_report::ScreenReport {
    tiler_core::world_report::ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(Rect {
            x,
            y: 0,
            w: 800,
            h: 600,
        }),
        work_area: Some(Rect {
            x,
            y: 0,
            w: 800,
            h: 560,
        }),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: output == "out-a",
    }
}

fn fact(window: &str, output: &str, frame: Rect) -> tiler_core::world_report::WindowFact {
    tiler_core::world_report::WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(frame),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
    }
}

fn parse_switch(token: &str) -> SwitchTarget {
    match token {
        "cursor-output" => SwitchTarget::Cursor,
        "focused-output" => SwitchTarget::Focused,
        other => panic!("unknown switch_target {other:?}"),
    }
}

fn parse_placement(token: &str) -> FreshPlacement {
    match token {
        "cursor-output-last-focused" => FreshPlacement::CursorOutputMru,
        "cursor-output-cursor-closest" => FreshPlacement::CursorOutputCursorClosest,
        "focused-output-last-focused" => FreshPlacement::FocusedOutputMru,
        other => panic!("unknown fresh_placement {other:?}"),
    }
}

struct Case {
    name: String,
    kind: String,
    switch_target: SwitchTarget,
    fresh_placement: FreshPlacement,
    seed_focus_order: Vec<String>,
    focus: String,
    cursor: (i32, i32),
    newcomer: Option<(String, String, Rect)>,
    command: Option<serde_json::Value>,
    expect: serde_json::Value,
}

fn parse_case(case: &serde_json::Value) -> Case {
    let cursor = case["cursor"].as_array().expect("cursor pair");
    let newcomer = case.get("newcomer").map(|n| {
        (
            n["id"].as_str().expect("newcomer id").to_owned(),
            n["native_output"]
                .as_str()
                .expect("native output")
                .to_owned(),
            rect(
                &n["rect"]
                    .as_array()
                    .expect("newcomer rect")
                    .iter()
                    .map(|v| v.as_i64().expect("i64"))
                    .collect::<Vec<_>>(),
            ),
        )
    });
    Case {
        name: case["name"].as_str().expect("case name").to_owned(),
        kind: case["kind"].as_str().expect("case kind").to_owned(),
        switch_target: parse_switch(case["settings"]["switch_target"].as_str().expect("switch")),
        fresh_placement: parse_placement(
            case["settings"]["fresh_placement"]
                .as_str()
                .expect("placement"),
        ),
        seed_focus_order: case["seed_focus_order"]
            .as_array()
            .expect("seed order")
            .iter()
            .map(|v| v.as_str().expect("focus id").to_owned())
            .collect(),
        focus: case["focus"].as_str().expect("focus").to_owned(),
        cursor: (
            cursor[0].as_i64().expect("x") as i32,
            cursor[1].as_i64().expect("y") as i32,
        ),
        newcomer,
        command: case.get("command").cloned(),
        expect: case["expect"].clone(),
    }
}

fn scene_frames(fixture: &serde_json::Value) -> Vec<(String, String, Rect)> {
    fixture["scene"]["windows"]
        .as_array()
        .expect("scene windows")
        .iter()
        .map(|w| {
            (
                w["id"].as_str().expect("window id").to_owned(),
                w["output"].as_str().expect("window output").to_owned(),
                rect(
                    &w["rect"]
                        .as_array()
                        .expect("window rect")
                        .iter()
                        .map(|v| v.as_i64().expect("i64"))
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    windows: Vec<tiler_core::world_report::WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    switch_target: SwitchTarget,
    fresh_placement: FreshPlacement,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        switch_target,
        fresh_placement,
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor,
        cursor_hit: None,
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn homed(reply: &tiler_core::world_report::WorldReply, window: &str) -> (String, String) {
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("{window} present"));
    (row.output.0.clone(), row.workspace.clone())
}

fn allocation(reply: &tiler_core::world_report::WorldReply, window: &str) -> Rect {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("{window} present"))
        .allocation
        .expect("tiled allocation")
}

fn view_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn union_box(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

fn build_command(
    command: &serde_json::Value,
    outputs: &std::collections::HashMap<String, String>,
) -> WorldCommand {
    match command["kind"].as_str().expect("command kind") {
        "select-trailing" => WorldCommand::SelectTrailing { output: None },
        "send-numbered" => WorldCommand::SendNumbered {
            window: WindowId(command["window"].as_str().expect("window").to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(OutputId(
                outputs
                    .get(command["window"].as_str().expect("window"))
                    .expect("mover output")
                    .clone(),
            )),
            index: command["index"].as_u64().expect("index") as u8,
            follow: command["follow"].as_bool().expect("follow"),
        },
        other => panic!("unknown command kind {other:?}"),
    }
}

#[test]
fn fresh_i3_retained_focus_covers_foreground_loss() {
    // Live defect: a newcomer that steals foreground (or arrives while the
    // adapter reports no/unknown focus) must still home on the retained
    // last-known focused output under focused-output-last-focused, not fall
    // back to primary. Baseline establishes retained out-b focus; variants
    // drive the arrival tick with degraded focus facts.
    let fixture = load_fixture();
    let scene = scene_frames(&fixture);
    let raw = fixture["cases"].as_array().expect("cases array");
    let ba = raw
        .iter()
        .find(|c| c["name"] == "fresh-i3-ba")
        .expect("fresh-i3-ba case");
    let case = parse_case(ba);
    assert_eq!(case.focus, "b1");
    assert_eq!(
        case.fresh_placement,
        FreshPlacement::FocusedOutputMru,
        "i3 placement"
    );
    let (new_id, native_output, native_rect) = case.newcomer.clone().expect("fresh newcomer");
    let scene_facts = || {
        scene
            .iter()
            .map(|(id, output, frame)| fact(id, output, *frame))
            .collect::<Vec<_>>()
    };
    let mut seq = 0u64;
    // One freshly seeded engine per variant: seed focus order leaves
    // retained focus on b1 (out-b) before the arrival tick.
    let seed_engine = |seq: &mut u64| {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let mut baseline = None;
        for focused in case
            .seed_focus_order
            .iter()
            .chain(std::iter::once(&case.focus))
        {
            *seq += 1;
            let reply = accepted(
                &mut engine,
                &report(
                    *seq,
                    scene_facts(),
                    focused,
                    Some(case.cursor),
                    case.switch_target,
                    case.fresh_placement,
                    None,
                    None,
                ),
            );
            assert!(reply.ready, "seed converges");
            baseline = Some(reply);
        }
        (engine, baseline.expect("baseline reply"))
    };
    let arrival_facts = |output: &str, frame: Rect| {
        let mut facts = scene_facts();
        facts.push(fact(&new_id, output, frame));
        facts
    };
    let admit = |engine: &mut WorldEngine,
                 seq: &mut u64,
                 facts: Vec<tiler_core::world_report::WindowFact>,
                 focused: &str| {
        *seq += 1;
        accepted(
            engine,
            &report(
                *seq,
                facts,
                focused,
                Some(case.cursor),
                case.switch_target,
                case.fresh_placement,
                None,
                None,
            ),
        )
    };

    // Empty global focus: latch (out-b) wins over primary fallback.
    let (mut engine, baseline) = seed_engine(&mut seq);
    let anchor_before = allocation(&baseline, "b1");
    let arrival = admit(
        &mut engine,
        &mut seq,
        arrival_facts(&native_output, native_rect),
        "",
    );
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-b",
        "empty focus still homes out-b"
    );
    assert_eq!(
        union_box(allocation(&arrival, &new_id), allocation(&arrival, "b1")),
        anchor_before,
        "empty focus splits the out-b anchor"
    );

    // Unknown focus token: same latch path.
    let (mut engine, _) = seed_engine(&mut seq);
    let arrival = admit(
        &mut engine,
        &mut seq,
        arrival_facts(&native_output, native_rect),
        "ghost-window",
    );
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-b",
        "unknown focus still homes out-b"
    );

    // Newcomer steals foreground with a primary native fact: the retained
    // pre-admission focus (out-b) wins over the brand-new native fact.
    let (mut engine, _) = seed_engine(&mut seq);
    let arrival = admit(
        &mut engine,
        &mut seq,
        arrival_facts(
            "out-a",
            Rect {
                x: 100,
                y: 100,
                w: 200,
                h: 150,
            },
        ),
        &new_id,
    );
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-b",
        "stealing newcomer does not reroute i3 source"
    );

    // Genuine managed focus elsewhere supersedes the latch.
    let (mut engine, _) = seed_engine(&mut seq);
    let arrival = admit(
        &mut engine,
        &mut seq,
        arrival_facts(&native_output, native_rect),
        "a1",
    );
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-a",
        "current managed focus on out-a wins"
    );

    // Disconnected topology filters the latch: out-b gone, primary wins.
    let (mut engine, _) = seed_engine(&mut seq);
    seq += 1;
    let removed_facts = vec![
        fact(
            "a1",
            "out-a",
            Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 560,
            },
        ),
        fact(
            "a2",
            "out-a",
            Rect {
                x: 400,
                y: 0,
                w: 400,
                h: 560,
            },
        ),
        fact(
            &new_id,
            "out-a",
            Rect {
                x: 100,
                y: 100,
                w: 200,
                h: 150,
            },
        ),
    ];
    let mut removed = report(
        seq,
        removed_facts,
        "",
        Some(case.cursor),
        case.switch_target,
        case.fresh_placement,
        None,
        None,
    );
    removed.screens = vec![screen("out-a", 0)];
    let arrival = accepted(&mut engine, &removed);
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-a",
        "vanished latch output is not picked"
    );

    // Quiet restart (new adapter incarnation) clears the latch.
    let (mut engine, _) = seed_engine(&mut seq);
    seq += 1;
    let mut reboot = report(
        seq,
        scene_facts(),
        "",
        Some(case.cursor),
        case.switch_target,
        case.fresh_placement,
        None,
        None,
    );
    reboot.adapter_incarnation = "adapter-2".to_owned();
    let bootstrapped = accepted(&mut engine, &reboot);
    assert!(bootstrapped.ready, "restart bootstraps");
    seq += 1;
    let mut arrival_report = report(
        seq,
        arrival_facts(
            "out-a",
            Rect {
                x: 100,
                y: 100,
                w: 200,
                h: 150,
            },
        ),
        "",
        Some(case.cursor),
        case.switch_target,
        case.fresh_placement,
        None,
        None,
    );
    arrival_report.adapter_incarnation = "adapter-2".to_owned();
    let arrival = accepted(&mut engine, &arrival_report);
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-a",
        "restart clears retained focus"
    );

    // Closing every window keeps the output context until topology removal.
    let (mut engine, _) = seed_engine(&mut seq);
    seq += 1;
    let mut closed = report(
        seq,
        vec![],
        "",
        Some(case.cursor),
        case.switch_target,
        case.fresh_placement,
        None,
        None,
    );
    closed.closed = vec![
        WindowId("a1".to_owned()),
        WindowId("a2".to_owned()),
        WindowId("b1".to_owned()),
        WindowId("b2".to_owned()),
    ];
    accepted(&mut engine, &closed);
    let arrival = admit(
        &mut engine,
        &mut seq,
        arrival_facts(&native_output, native_rect),
        "",
    );
    assert_eq!(
        homed(&arrival, &new_id).0,
        "out-b",
        "all-closed keeps last-focused output"
    );

    // Cursor policy is unaffected by an empty focus fact.
    let cosmic = raw
        .iter()
        .find(|c| c["name"] == "fresh-cosmic-ba")
        .expect("fresh-cosmic-ba case");
    let cosmic = parse_case(cosmic);
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    for focused in cosmic
        .seed_focus_order
        .iter()
        .chain(std::iter::once(&cosmic.focus))
    {
        seq += 1;
        accepted(
            &mut engine,
            &report(
                seq,
                scene_facts(),
                focused,
                Some(cosmic.cursor),
                cosmic.switch_target,
                cosmic.fresh_placement,
                None,
                None,
            ),
        );
    }
    let (cosmic_new, cosmic_native, cosmic_rect) = cosmic.newcomer.clone().expect("newcomer");
    let mut facts = scene_facts();
    facts.push(fact(&cosmic_new, &cosmic_native, cosmic_rect));
    seq += 1;
    let arrival = accepted(
        &mut engine,
        &report(
            seq,
            facts,
            "",
            Some(cosmic.cursor),
            cosmic.switch_target,
            cosmic.fresh_placement,
            None,
            None,
        ),
    );
    assert_eq!(
        homed(&arrival, &cosmic_new).0,
        "out-a",
        "cursor policy still wins with empty focus"
    );
}

#[test]
fn fresh_focus_trace_labels_cached_leg_not_current() {
    // The cached leg trace must never pose as the current report's focus
    // fact: after baseline (non-admission) reports it stays default, and
    // after an admission it names the decision inputs. The adapter pairs
    // it with the current token under `last_focused_leg`.
    let fixture = load_fixture();
    let scene = scene_frames(&fixture);
    let raw = fixture["cases"].as_array().expect("cases array");
    let ba = raw
        .iter()
        .find(|c| c["name"] == "fresh-i3-ba")
        .expect("fresh-i3-ba case");
    let case = parse_case(ba);
    let scene_facts = || {
        scene
            .iter()
            .map(|(id, output, frame)| fact(id, output, *frame))
            .collect::<Vec<_>>()
    };
    let mut seq = 0u64;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    for focused in case
        .seed_focus_order
        .iter()
        .chain(std::iter::once(&case.focus))
    {
        seq += 1;
        accepted(
            &mut engine,
            &report(
                seq,
                scene_facts(),
                focused,
                Some(case.cursor),
                case.switch_target,
                case.fresh_placement,
                None,
                None,
            ),
        );
    }
    // Baseline only: no admission ran, so the cached leg is default even
    // though the current report carries focus. It must not claim current.
    assert_eq!(
        engine.fresh_focus_trace(),
        (None, None, None, "primary"),
        "non-admission reports leave the cached leg default"
    );
    // Empty-focus admission: the leg records retained routing, not a
    // current-focus claim.
    let (new_id, native_output, native_rect) = case.newcomer.clone().expect("newcomer");
    let mut facts = scene_facts();
    facts.push(fact(&new_id, &native_output, native_rect));
    seq += 1;
    accepted(
        &mut engine,
        &report(
            seq,
            facts,
            "",
            Some(case.cursor),
            case.switch_target,
            case.fresh_placement,
            None,
            None,
        ),
    );
    assert_eq!(
        engine.fresh_focus_trace(),
        (
            None,
            Some("out-b".to_owned()),
            Some("out-b".to_owned()),
            "retained"
        ),
        "empty-focus admission records the retained leg"
    );
    // Current managed focus: the leg records current inputs distinctly
    // from the retained latch.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    for focused in case
        .seed_focus_order
        .iter()
        .chain(std::iter::once(&case.focus))
    {
        seq += 1;
        accepted(
            &mut engine,
            &report(
                seq,
                scene_facts(),
                focused,
                Some(case.cursor),
                case.switch_target,
                case.fresh_placement,
                None,
                None,
            ),
        );
    }
    let mut facts = scene_facts();
    facts.push(fact(&new_id, &native_output, native_rect));
    seq += 1;
    accepted(
        &mut engine,
        &report(
            seq,
            facts,
            "a1",
            Some(case.cursor),
            case.switch_target,
            case.fresh_placement,
            None,
            None,
        ),
    );
    assert_eq!(
        engine.fresh_focus_trace(),
        (
            Some("out-a".to_owned()),
            Some("out-b".to_owned()),
            Some("out-a".to_owned()),
            "current"
        ),
        "managed focus admission records current over retained"
    );
}

#[test]
fn shared_world_actions_converge_per_fixture() {
    let fixture = load_fixture();
    let scene = scene_frames(&fixture);
    // Scene outputs double as the Windows device map (out-a -> DEV1,
    // out-b -> DEV2); the live connector scales from the same ids.
    let mover_outputs: std::collections::HashMap<String, String> = scene
        .iter()
        .map(|(id, output, _)| (id.clone(), output.clone()))
        .collect();
    let cases = fixture["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "fixture carries action cases");
    for raw in cases {
        let case = parse_case(raw);
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let mut seq = 0u64;
        let mut next_report = |windows: Vec<tiler_core::world_report::WindowFact>,
                               focused: &str,
                               command: Option<(u64, WorldCommand)>| {
            seq += 1;
            let (command_seq, cmd) = command
                .map(|(s, c)| (Some(s), Some(c)))
                .unwrap_or((None, None));
            report(
                seq,
                windows,
                focused,
                Some(case.cursor),
                case.switch_target,
                case.fresh_placement,
                command_seq,
                cmd,
            )
        };
        let scene_facts = || {
            scene
                .iter()
                .map(|(id, output, frame)| fact(id, output, *frame))
                .collect::<Vec<_>>()
        };
        for focused in &case.seed_focus_order {
            let reply = accepted(&mut engine, &next_report(scene_facts(), focused, None));
            assert!(reply.ready, "{}: seed converges", case.name);
        }
        match case.kind.as_str() {
            "fresh" => {
                let (new_id, native_output, native_rect) =
                    case.newcomer.clone().expect("fresh newcomer");
                let baseline =
                    accepted(&mut engine, &next_report(scene_facts(), &case.focus, None));
                let mut arrival_facts = scene_facts();
                arrival_facts.push(fact(&new_id, &native_output, native_rect));
                let arrival = accepted(&mut engine, &next_report(arrival_facts, &case.focus, None));
                let expect_output = case.expect["output"].as_str().expect("expect output");
                let anchor = case.expect["anchor"].as_str().expect("expect anchor");
                let stable = case.expect["stable_peer"].as_str().expect("stable peer");
                // Policy output wins over native affinity.
                assert_eq!(
                    homed(&arrival, &new_id).0,
                    expect_output,
                    "{}: newcomer output",
                    case.name
                );
                assert!(
                    arrival.effects.iter().any(|e| e.target.0 == new_id),
                    "{}: newcomer focuses",
                    case.name
                );
                // Exactly the anchor shrinks; the stable peer is untouched.
                let anchor_before = allocation(&baseline, anchor);
                let anchor_after = allocation(&arrival, anchor);
                assert!(
                    anchor_after.w < anchor_before.w || anchor_after.h < anchor_before.h,
                    "{}: anchor {anchor} shrinks {anchor_before:?} -> {anchor_after:?}",
                    case.name
                );
                assert_eq!(
                    allocation(&arrival, stable),
                    allocation(&baseline, stable),
                    "{}: stable peer {stable} unchanged",
                    case.name
                );
                // The newcomer shares the anchor's old region exactly.
                let newcomer_alloc = allocation(&arrival, &new_id);
                assert_eq!(
                    union_box(newcomer_alloc, anchor_after),
                    anchor_before,
                    "{}: newcomer {newcomer_alloc:?} plus anchor {anchor_after:?} refill {anchor_before:?}",
                    case.name
                );
                // Listed source windows keep their tiles.
                for window in case.expect["source_unchanged"]
                    .as_array()
                    .expect("source_unchanged")
                {
                    let id = window.as_str().expect("window id");
                    assert_eq!(
                        allocation(&arrival, id),
                        allocation(&baseline, id),
                        "{}: source window {id} unchanged",
                        case.name
                    );
                }
                // Target view proves and the next observation settles.
                let view = view_of(&arrival, expect_output).expect("target view");
                assert_eq!(
                    homed(&arrival, &new_id).1,
                    view,
                    "{}: newcomer homed on the proven view",
                    case.name
                );
                let mut settled_facts = scene_facts();
                settled_facts.push(fact(&new_id, expect_output, newcomer_alloc));
                let settled = accepted(&mut engine, &next_report(settled_facts, &case.focus, None));
                assert!(
                    settled.transfers.is_empty(),
                    "{}: settles with no flights: {:?}",
                    case.name,
                    settled.transfers
                );
                assert_eq!(
                    homed(&settled, &new_id).0,
                    expect_output,
                    "{}: homing retained",
                    case.name
                );
            }
            "switch" => {
                let baseline =
                    accepted(&mut engine, &next_report(scene_facts(), &case.focus, None));
                let expect_output = case.expect["output"].as_str().expect("expect output");
                let other = case.expect["other_output"].as_str().expect("other output");
                let other_before = view_of(&baseline, other).expect("other view");
                let reply = accepted(
                    &mut engine,
                    &next_report(
                        scene_facts(),
                        &case.focus,
                        Some((
                            11,
                            build_command(case.command.as_ref().expect("command"), &mover_outputs),
                        )),
                    ),
                );
                let after = view_of(&reply, expect_output).expect("target view");
                let before = view_of(&baseline, expect_output).expect("baseline view");
                assert_ne!(
                    after, before,
                    "{}: trailing lands on {expect_output}",
                    case.name
                );
                assert_eq!(
                    view_of(&reply, other).as_deref(),
                    Some(other_before.as_str()),
                    "{}: {other} untouched",
                    case.name
                );
                assert!(
                    reply
                        .transfers
                        .iter()
                        .any(|t| t.target_output.0 == expect_output),
                    "{}: view flight names {expect_output}: {:?}",
                    case.name,
                    reply.transfers
                );
            }
            "send" => {
                let command =
                    build_command(case.command.as_ref().expect("command"), &mover_outputs);
                let mover = case.command.as_ref().expect("command")["window"]
                    .as_str()
                    .expect("mover")
                    .to_owned();
                let expect_output = case.expect["output"].as_str().expect("expect output");
                let reply = accepted(
                    &mut engine,
                    &next_report(scene_facts(), &case.focus, Some((11, command))),
                );
                assert!(
                    reply
                        .transfers
                        .iter()
                        .filter(|t| matches!(&t.kind, tiler_core::world_report::TransferKind::Window(w) if w.0 == mover))
                        .all(|t| t.target_output.0 == expect_output),
                    "{}: send resolves on the mover output: {:?}",
                    case.name, reply.transfers
                );
            }
            other => panic!("unknown case kind {other:?}"),
        }
    }
}
