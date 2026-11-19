//! Offline release scale harness for `WorldEngine::handle_report` cost.
//!
//! Builds typed v4 reports in memory (no protocol decode in the timed path),
//! converges each engine against echoed desired geometry and echoed focus,
//! then measures the first report on a fresh engine plus three shapes: S1 the
//! unchanged converged report, S2 the same report with alternating win-0
//! width/height minimum hints, and S3 a one-command accepted orientation
//! toggle. Each shape owns a fresh engine and repeats the full convergence
//! oracle before warmup/measure.
//!
//! Every timed call carries a fresh monotonic request sequence, observation
//! sequence and request id, so duplicate-reply caching cannot mask cost.
//! Reports are prebuilt and only their identity fields (and, for S2, the win-0
//! hint axis, for S3, the queued command sequence) change between samples. All
//! oracle checks run outside the timed `handle_report` call.
//!
//! Usage: report_scale [filter] [samples] [reps] [shape]
//!   filter  exact case name, otherwise substring, or "all" (default "all")
//!   samples measured calls per shape (default 24, bounded 1..=1000)
//!   reps    ordered case rotations (default 1, bounded 1..=8)
//!   shape   S1 (default), S2, S3, or all (exact, case-sensitive)

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ActivationPolicy, BatchSettlement, NativeObservation, NativeOccupancy, NativeWorkspaceFact,
    OutputIdentity, PresentationIngressV4, PresentationRowV4, ScreenReport, VisibilityFact,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WindowType, WorldCommand, WorldReportV4,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    S1,
    S2,
    S3,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Self::S1 => "S1 unchanged",
            Self::S2 => "S2 hints changed",
            Self::S3 => "S3 command accepted",
        }
    }
}

fn parse_shapes(raw: &str) -> Vec<Shape> {
    match raw {
        "S1" => vec![Shape::S1],
        "S2" => vec![Shape::S2],
        "S3" => vec![Shape::S3],
        "all" => vec![Shape::S1, Shape::S2, Shape::S3],
        other => panic!("invalid shape {other:?}; expected S1, S2, S3 or all"),
    }
}

// Per-output rectangles tile horizontally inside the shared geometry bound
// (x within [-16384, 16384], each dimension <= 16384). Four outputs use
// x = 0, 4100, 8200, 12300 with width 4000, ending at 16300.
fn screen(output: &str, index: usize) -> ScreenReport {
    let x = (index as i32) * 4100;
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 4000, 4000)),
        work_area: Some(rect(x, 0, 4000, 4000)),
        complete: true,
        scale: 1.0,
        identity: OutputIdentity::unknown(),
    }
}

fn fact(window: &str, output: &str, x: i32, y: i32, native: Option<&str>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(x, y, 100, 100)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: match native {
            Some(id) => NativeWorkspaceFact::Value(id.to_owned()),
            None => NativeWorkspaceFact::Omitted,
        },
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(WindowType::Normal),
        activation_policy: Some(ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
    }
}

// Occupancy counts are derived from the actual window-to-native assignment,
// never hard-coded: every ordered desktop reports how many windows it holds.
fn native_obs(output: &str, order: &[String], counts: &[usize]) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.to_vec(),
        current_native: order.first().cloned(),
        occupancy: order
            .iter()
            .zip(counts)
            .map(|(native, count)| NativeOccupancy {
                native: native.clone(),
                count: Some(*count as u32),
            })
            .collect(),
    }
}

fn report(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
    focused: &str,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native,
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

#[derive(Clone)]
struct Case {
    name: String,
    windows: usize,
    outputs: usize,
    // Workspaces per output. Zero uses the default logical ring (no native
    // domains); nonzero bootstraps that many occupied native domains.
    workspaces: usize,
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for windows in [10, 50, 200] {
        for outputs in [1, 2, 4] {
            out.push(Case {
                name: format!("w{windows}-o{outputs}"),
                windows,
                outputs,
                workspaces: 0,
            });
        }
    }
    for workspaces in [2, 10, 20] {
        out.push(Case {
            name: format!("w50-o2-ws{workspaces}"),
            windows: 50,
            outputs: 2,
            workspaces,
        });
    }
    out
}

fn engine_for(case: &Case) -> WorldEngine {
    if case.workspaces == 0 {
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
    } else {
        WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation")
    }
}

fn native_names(case: &Case) -> Vec<Vec<String>> {
    (0..case.outputs)
        .map(|o| {
            (0..case.workspaces)
                .map(|n| format!("native-{o}-{n}"))
                .collect()
        })
        .collect()
}

fn seed_report(case: &Case) -> WorldReportV4 {
    let screens: Vec<_> = (0..case.outputs)
        .map(|o| screen(&format!("out-{o}"), o))
        .collect();
    let natives = native_names(case);
    let mut counts: Vec<Vec<usize>> = vec![vec![0; case.workspaces]; case.outputs];
    let mut windows = Vec::new();
    for k in 0..case.windows {
        let o = k % case.outputs;
        let native_id = if natives[o].is_empty() {
            None
        } else {
            // Spread windows over the occupied domains on this output.
            let slot = (k / case.outputs) % natives[o].len();
            counts[o][slot] += 1;
            Some(natives[o][slot].as_str())
        };
        let x = (o as i32) * 4100 + 4;
        let y = ((k / case.outputs) as i32 % 100) * 60 + 4;
        windows.push(fact(
            &format!("win-{k}"),
            &format!("out-{o}"),
            x,
            y,
            native_id,
        ));
    }
    let native: Vec<_> = (0..case.outputs)
        .filter(|&o| !natives[o].is_empty())
        .map(|o| native_obs(&format!("out-{o}"), &natives[o], &counts[o]))
        .collect();
    report(1, screens, windows, native, "win-0")
}

type WorldReply = tiler_core::world_report::WorldReplyV4;

fn allocations(reply: &WorldReply) -> Vec<(String, Option<Rect>)> {
    let mut rows: Vec<_> = reply
        .windows
        .iter()
        .map(|w| (w.lifetime.0.clone(), w.allocation))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

// S2 alternates win-0's single meaningful minimum between width and height.
// A positive 1 is below every tiled extent, so the desired allocation cannot
// change; every call still carries a meaningful hint and differs from the
// previous call, so an unhinted-only skip cannot hide the hinted path.
fn set_win0_hint(r: &mut WorldReportV4, case: &Case, width: bool) {
    let hints = if width {
        WindowSizeHints {
            min_w: Some(1),
            ..WindowSizeHints::none()
        }
    } else {
        WindowSizeHints {
            min_h: Some(1),
            ..WindowSizeHints::none()
        }
    };
    let w = r
        .windows
        .iter_mut()
        .find(|w| w.window.0 == "win-0")
        .unwrap_or_else(|| panic!("{}: win-0 missing from report", case.name));
    w.hints = Some(hints);
}

fn assert_all_tiled_positive(case: &Case, reply: &WorldReply) {
    assert_eq!(
        reply.windows.len(),
        case.windows,
        "{}: reply window rows {} != {}",
        case.name,
        reply.windows.len(),
        case.windows
    );
    for w in &reply.windows {
        assert!(!w.floating, "{}: {} floated", case.name, w.lifetime.0);
        match w.allocation {
            Some(r) => assert!(
                r.w > 0 && r.h > 0,
                "{}: {} nonpositive tile {r:?}",
                case.name,
                w.lifetime.0
            ),
            None => panic!("{}: {} has no allocation", case.name, w.lifetime.0),
        }
    }
}

// Number of seeded windows sharing win-0's initial tiling domain. The toggle
// needs at least two members there to change the desired topology. Native-free
// cases group by output; host-owned cases group by retained native homing.
fn domain_size(case: &Case, engine: &WorldEngine, seed: &WorldReportV4) -> usize {
    let win0 = WindowId("win-0".to_owned());
    if case.workspaces == 0 {
        let out = seed
            .windows
            .iter()
            .find(|w| w.window == win0)
            .expect("win-0 seeded")
            .output
            .clone()
            .expect("win-0 output");
        seed.windows
            .iter()
            .filter(|w| w.output == Some(out.clone()))
            .count()
    } else {
        let home = engine.adapter_homing(&win0).expect("win-0 homing");
        seed.windows
            .iter()
            .filter(|w| engine.adapter_homing(&w.window) == Some(home.clone()))
            .count()
    }
}

// Full convergence oracle. Runs outside the timed region.
fn assert_converged(case: &Case, engine: &WorldEngine, r: &WorldReportV4, reply: &WorldReply) {
    let alloc: BTreeMap<&str, Option<Rect>> = reply
        .windows
        .iter()
        .map(|w| (w.lifetime.0.as_str(), w.allocation))
        .collect();

    assert_eq!(
        reply.windows.len(),
        case.windows,
        "{}: reply window rows {} != {}",
        case.name,
        reply.windows.len(),
        case.windows
    );
    assert!(
        reply
            .windows
            .iter()
            .all(|w| !w.floating && w.allocation.is_some()),
        "{}: a row floats or has no allocation",
        case.name
    );

    // Every observed fact rect is exactly the allocation returned for it.
    for w in &r.windows {
        let target = alloc
            .get(w.window.0.as_str())
            .unwrap_or_else(|| panic!("{}: no allocation for {}", case.name, w.window.0));
        assert_eq!(
            w.rect, *target,
            "{}: observed rect for {} is not the returned allocation",
            case.name, w.window.0
        );
    }

    // Every retained member is present, and only those members exist.
    let members = engine.logical_members_all();
    let member_set: BTreeSet<String> = members.into_iter().map(|w| w.0).collect();
    let seed_set: BTreeSet<String> = r.windows.iter().map(|w| w.window.0.clone()).collect();
    assert_eq!(
        member_set, seed_set,
        "{}: retained members differ from seeded windows",
        case.name
    );

    // No transfer or focus request is outstanding at the measured start.
    assert!(
        reply.transfers.is_empty(),
        "{}: outstanding transfer at convergence",
        case.name
    );
    assert!(
        reply.focus_request.is_none(),
        "{}: outstanding focus request at convergence",
        case.name
    );

    if case.workspaces > 0 {
        // Occupancy counts are truthful against the observed facts.
        let mut observed: BTreeMap<(String, String), usize> = BTreeMap::new();
        for w in &r.windows {
            if let NativeWorkspaceFact::Value(id) = &w.native_workspace {
                let out = w
                    .output
                    .as_ref()
                    .expect("native fact has an output")
                    .0
                    .clone();
                *observed.entry((out, id.clone())).or_insert(0) += 1;
            }
        }
        for row in &r.native {
            for occ in &row.occupancy {
                let expected = observed
                    .get(&(row.output.0.clone(), occ.native.clone()))
                    .copied()
                    .unwrap_or(0);
                assert_eq!(
                    occ.count,
                    Some(expected as u32),
                    "{}: occupancy for {}/{} is not the real window count",
                    case.name,
                    row.output.0,
                    occ.native
                );
            }
        }

        // Exactly the configured number of logical domains per output are
        // actually occupied (empty trailing domains are not counted).
        for o in 0..case.outputs {
            let out = format!("out-{o}");
            let mut occupied: BTreeSet<String> = BTreeSet::new();
            for w in &member_set {
                if let Some((home_out, workspace)) = engine.adapter_homing(&WindowId(w.clone()))
                    && home_out == out
                {
                    occupied.insert(workspace);
                }
            }
            assert_eq!(
                occupied.len(),
                case.workspaces,
                "{}: {out} has {} occupied domains, expected {}",
                case.name,
                occupied.len(),
                case.workspaces
            );
        }
    }
}

/// Converge by echoing desired allocations and the requested focus target
/// back as observations. A pending focus retires on observed-target equality
/// (verified in `retire_pending_focus`); no synthetic feedback is fabricated.
fn converge(
    case: &Case,
    engine: &mut WorldEngine,
    mut r: WorldReportV4,
) -> (WorldReportV4, WorldReply) {
    let mut prev: Option<Vec<(String, Option<Rect>)>> = None;
    for round in 0..32 {
        let reply = match engine.handle_report(&r, &[]).0 {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        let alloc = allocations(&reply);
        let stable = prev.as_ref() == Some(&alloc)
            && reply.transfers.is_empty()
            && reply.focus_request.is_none();
        if stable {
            assert_converged(case, engine, &r, &reply);
            return (r, reply);
        }
        let next_seq = r.request_seq + 1;
        let mut next = r.clone();
        next.request_seq = next_seq;
        next.observation_seq = next_seq;
        next.request_id = format!("req-{next_seq}");
        for w in &mut next.windows {
            if let Some((_, Some(target))) = alloc.iter().find(|(id, _)| id == &w.window.0) {
                w.rect = Some(*target);
            }
        }
        if let Some(request) = &reply.focus_request {
            next.focused_global = request.target.0.clone();
            next.foreground_known = true;
        }
        prev = Some(alloc);
        r = next;
        assert!(round < 31, "{}: did not converge", case.name);
    }
    unreachable!()
}

fn bump(r: &mut WorldReportV4, seq: &mut u64) {
    *seq += 1;
    r.request_seq = *seq;
    r.observation_seq = *seq;
    r.request_id = format!("req-{}", *seq);
}

#[inline(never)]
fn call(engine: &mut WorldEngine, r: &WorldReportV4, expected: &[(String, Option<Rect>)]) -> f64 {
    let before = engine.state_revision();
    let start = Instant::now();
    let outcome = engine.handle_report(r, &[]).0;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    // A fresh request must advance the revision; a cached duplicate would
    // leave it unchanged and silently hide the reconciliation cost.
    assert!(
        engine.state_revision() > before,
        "report {} did not advance state_revision",
        r.request_id
    );
    // Steady-shape oracle, checked outside the timed call.
    assert_eq!(
        allocations(&reply),
        expected,
        "{}: allocations changed on an unchanged report",
        r.request_id
    );
    assert!(
        reply.transfers.is_empty() && reply.focus_request.is_none(),
        "{}: outstanding transfer or focus after an unchanged report",
        r.request_id
    );
    elapsed
}

// S2 wrapper: the hint-axis toggle is an input assertion; `call` is the shared
// steady boundary (S1 by default, S2 when this shape is selected), so no S2
// sample carries an unhinted report.
#[inline(never)]
fn call_hint(
    engine: &mut WorldEngine,
    r: &WorldReportV4,
    expected: &[(String, Option<Rect>)],
    width: bool,
    previous: Option<bool>,
) -> f64 {
    if let Some(prev) = previous {
        assert!(
            width != prev,
            "S2: hint axis did not toggle from the previous call"
        );
    }
    call(engine, r, expected)
}

// S3 boundary: only handle_report is timed. The reply and settlements leave
// the wrapper so every oracle runs outside the timed region.
#[inline(never)]
fn call_command(
    engine: &mut WorldEngine,
    r: &WorldReportV4,
    queued: &[(u64, WorldCommand)],
) -> (f64, WorldReply, Vec<BatchSettlement>) {
    let before = engine.state_revision();
    let start = Instant::now();
    let (outcome, settlements) = engine.handle_report(r, queued);
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        engine.state_revision() > before,
        "report {} did not advance state_revision",
        r.request_id
    );
    (elapsed, reply, settlements)
}

fn median_p95(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();
    let median = values[n / 2];
    let p95_index = ((n as f64) * 0.95).ceil() as usize - 1;
    (median, values[p95_index.min(n - 1)])
}

// Returns (first_report_ms, median_ms, p95_ms, final_state_revision). Every
// shape gets its own first-report engine and its own converged measurement
// engine, so no shape borrows another shape's warm state.
fn measure_case(case: &Case, samples: usize, shape: Shape) -> (f64, f64, f64, u64) {
    let mut first_engine = engine_for(case);
    let first_ms = {
        let seed = seed_report(case);
        let before = first_engine.state_revision();
        let start = Instant::now();
        let outcome = first_engine.handle_report(&seed, &[]).0;
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        match outcome {
            WorldOutcome::Accepted(_) => {}
            other => panic!("expected accepted, got {other:?}"),
        }
        assert!(
            first_engine.state_revision() > before,
            "first report did not advance state_revision"
        );
        ms
    };

    let mut engine = engine_for(case);
    let (converged, reply) = converge(case, &mut engine, seed_report(case));
    let expected = allocations(&reply);
    let mut seq = converged.request_seq;

    let (median, p95) = match shape {
        Shape::S1 => {
            // Warm up and confirm the steady oracle before measuring.
            for _ in 0..3 {
                let mut warm = converged.clone();
                bump(&mut warm, &mut seq);
                let _ = call(&mut engine, &warm, &expected);
            }
            let mut times = Vec::with_capacity(samples);
            for _ in 0..samples {
                let mut r = converged.clone();
                bump(&mut r, &mut seq);
                times.push(call(&mut engine, &r, &expected));
            }
            median_p95(times)
        }
        Shape::S2 => {
            // First changed call uses min_w; every later call flips the axis.
            let mut previous: Option<bool> = None;
            let mut warmups = 0;
            let mut times = Vec::with_capacity(3 + samples);
            for _ in 0..(3 + samples) {
                let mut r = converged.clone();
                bump(&mut r, &mut seq);
                let width = !previous.unwrap_or(false);
                set_win0_hint(&mut r, case, width);
                let ms = call_hint(&mut engine, &r, &expected, width, previous);
                previous = Some(width);
                if warmups < 3 {
                    warmups += 1;
                } else {
                    times.push(ms);
                }
            }
            median_p95(times)
        }
        Shape::S3 => {
            let size = domain_size(case, &engine, &converged);
            assert!(
                size >= 2,
                "{}: win-0 domain has {size} seeded windows, expected at least 2",
                case.name
            );
            if case.workspaces == 20 {
                assert_eq!(size, 2, "{}: ws20 win-0 domain has {size} != 2", case.name);
            }

            let mut base = converged.clone();
            let mut current = expected.clone();
            let mut cmd_seq = 0u64;
            let mut times = Vec::with_capacity(3 + samples);
            // Three command+settlement cycles warm up the same path; every
            // oracle runs for them too, but only later cycles are timed samples.
            for iteration in 0..(3 + samples) {
                bump(&mut base, &mut seq);
                cmd_seq += 1;
                assert!(
                    base.command.is_none(),
                    "{}: report carries inline command",
                    case.name
                );
                let queued = [(
                    cmd_seq,
                    WorldCommand::Operation(CoreCommand::ToggleOrientation {
                        window: tiler_core::directional::WindowSubject::Window(
                            tiler_core::directional::WindowId("win-0".to_owned()),
                        ),
                    }),
                )];
                let (ms, reply, settlements) = call_command(&mut engine, &base, &queued);

                assert_eq!(
                    settlements.len(),
                    1,
                    "{}: expected one settlement",
                    case.name
                );
                assert_eq!(
                    settlements[0].seq, cmd_seq,
                    "{}: settlement seq mismatch",
                    case.name
                );
                assert!(
                    settlements[0].result.is_none(),
                    "{}: command did not settle ready: {:?}",
                    case.name,
                    settlements[0].result
                );
                assert!(
                    reply.transfers.is_empty() && reply.focus_request.is_none(),
                    "{}: outstanding transfer or focus after toggle",
                    case.name
                );
                assert_all_tiled_positive(case, &reply);
                let after = allocations(&reply);
                assert_ne!(
                    after, current,
                    "{}: toggle did not change the desired allocation",
                    case.name
                );
                if iteration >= 3 {
                    times.push(ms);
                }

                // Commandless settlement, outside the timed wrapper: echo the
                // desired geometry as observations, then require the full
                // convergence oracle with stable allocations and no action.
                bump(&mut base, &mut seq);
                let mut settled = base.clone();
                for w in &mut settled.windows {
                    if let Some((_, Some(target))) = after.iter().find(|(id, _)| id == &w.window.0)
                    {
                        w.rect = Some(*target);
                    }
                }
                assert!(
                    settled.command.is_none(),
                    "{}: settlement has command",
                    case.name
                );
                let (outcome, extra) = engine.handle_report(&settled, &[]);
                assert!(
                    extra.is_empty(),
                    "{}: commandless report produced settlements",
                    case.name
                );
                let settled_reply = match outcome {
                    WorldOutcome::Accepted(reply) => reply,
                    other => panic!("expected accepted settlement, got {other:?}"),
                };
                let settled_alloc = allocations(&settled_reply);
                assert_eq!(
                    settled_alloc, after,
                    "{}: allocations not stable after settlement",
                    case.name
                );
                assert!(
                    settled_reply.transfers.is_empty() && settled_reply.focus_request.is_none(),
                    "{}: outstanding action after settlement",
                    case.name
                );
                assert_converged(case, &engine, &settled, &settled_reply);
                current = settled_alloc;
                base = settled;
            }
            median_p95(times)
        }
    };
    (first_ms, median, p95, engine.state_revision())
}

fn bounded(name: &str, raw: &str, default: usize, min: usize, max: usize) -> usize {
    match raw.parse::<usize>() {
        Ok(v) => v.clamp(min, max),
        Err(_) => {
            eprintln!("invalid {name}: {raw}; using {default}");
            default
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let filter = args.first().map(String::as_str).unwrap_or("all");
    let samples = bounded(
        "samples",
        args.get(1).map(String::as_str).unwrap_or("24"),
        24,
        1,
        1000,
    );
    let reps = bounded(
        "reps",
        args.get(2).map(String::as_str).unwrap_or("1"),
        1,
        1,
        8,
    );
    let shape_arg = args.get(3).map(String::as_str).unwrap_or("S1");
    let shapes = parse_shapes(shape_arg);

    let all = cases();
    let exact = all.iter().any(|case| case.name == filter);
    let selected: Vec<Case> = if filter == "all" {
        all
    } else {
        all.into_iter()
            .filter(|c| {
                if exact {
                    c.name == filter
                } else {
                    c.name.contains(filter)
                }
            })
            .collect()
    };
    assert!(!selected.is_empty(), "no case matched filter {filter:?}");

    println!(
        "report_scale: samples={samples} reps={reps} filter={filter} shape={shape_arg} selected={}",
        selected.len()
    );
    println!(
        "conditions: typed handle_report; reports prebuilt (no decode timed); fresh seq/obs/request_id per call; fresh engine per case and shape; work-area 4000x4000 per output; native occupancy counts derived from real window assignment"
    );
    println!("case,shape,median_ms,p95_ms,first_report_ms,state_revision");

    for rep in 0..reps {
        // Deterministic rotation to reduce fixed-order bias.
        let mut order = selected.clone();
        let shift = rep % order.len();
        order.rotate_left(shift);
        for case in &order {
            for shape in &shapes {
                let (first, median, p95, rev) = measure_case(case, samples, *shape);
                println!(
                    "{},{},{:.4},{:.4},{:.4},{}",
                    case.name,
                    shape.label(),
                    median,
                    p95,
                    first,
                    rev
                );
            }
        }
    }
}
