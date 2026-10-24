//! Core-only narrow acceptance runner (development-only, never a public API).
//!
//! One tiled-only, zero-gap, per-output-local case: bootstrap two windows,
//! admit a third, select-away, select-previous return. Runs the actual
//! [`WorldEngine`] on authored host inputs and checks the narrow accepted
//! result: desired roster/homing/authorities, actual allocations with
//! work-area coverage, oracle tiles, effective frames, output views,
//! workspace ring/orders/scopes, retained view flights, one-shot effects
//! (id/kind/target plus issued revision), settlement status, and
//! report/reply freshness and pairing. Usage: `world_acceptance CASE`.
//!
//! No transcript mode, no native rows, no caller bindings: host visibility
//! is not observed by the core and no live claim is made. Command steps pin
//! exactly one retained view flight (`settlement: command-pending`) and
//! settle on their receipt steps (`settlement: settled`); omitting a receipt
//! leaves the flight outstanding and fails. Unsupported classes (float,
//! sticky, fullscreen, maximize, minimize, constraints, gaps, host-owned,
//! gestures, cursor/warp) are rejected as not covered, never held.
//! Presentation shapes are explicitly out of scope (never read).

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ScreenReport, ScreenView, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReply,
    WorldReport,
};

// ---------------------------------------------------------------------------
// Case file schema (strict: unknown fields refuse).
// ---------------------------------------------------------------------------

fn rect4(v: [i32; 4]) -> Rect {
    Rect {
        x: v[0],
        y: v[1],
        w: v[2],
        h: v[3],
    }
}

fn rect_of(v: Rect) -> [i32; 4] {
    [v.x, v.y, v.w, v.h]
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseScreen {
    out: String,
    full: [i32; 4],
    work: [i32; 4],
    gap: i32,
    outer_gap: i32,
    complete: bool,
    primary: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseSettings {
    default_tiled: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseCommand {
    seq: u64,
    op: String,
    output: Option<String>,
    workspace: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseTransfer {
    op: String,
    kind: String,
    window: Option<String>,
    output: String,
    workspace: Option<String>,
    follow: bool,
    held: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseEffect {
    id: String,
    kind: String,
    target: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseExpect {
    view: String,
    members: Vec<String>,
    tiles: BTreeMap<String, [i32; 4]>,
    transfers: Vec<CaseTransfer>,
    effects: Vec<CaseEffect>,
    settlement: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseStep {
    name: String,
    request: u64,
    observation: u64,
    facts: BTreeMap<String, [i32; 4]>,
    views: BTreeMap<String, String>,
    command: Option<CaseCommand>,
    expect: CaseExpect,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    name: String,
    #[allow(dead_code)]
    description: String,
    owner: String,
    adapter: String,
    generation: String,
    mode: String,
    screens: Vec<CaseScreen>,
    settings: CaseSettings,
    windows: Vec<String>,
    steps: Vec<CaseStep>,
}

fn load_case(path: &str) -> Result<CaseFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read case: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse case: {e}"))
}

// ---------------------------------------------------------------------------
// Geometry: coverage is proven over ACTUAL reply allocations.
// ---------------------------------------------------------------------------

fn area(r: [i32; 4]) -> i64 {
    i64::from(r[2]) * i64::from(r[3])
}

fn overlap(a: [i32; 4], b: [i32; 4]) -> bool {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    (a[0] + a[2]).min(b[0] + b[2]) > x0 && (a[1] + a[3]).min(b[1] + b[3]) > y0
}

fn inside(outer: [i32; 4], inner: [i32; 4]) -> bool {
    inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[0] + inner[2] <= outer[0] + outer[2]
        && inner[1] + inner[3] <= outer[1] + outer[3]
}

fn coverage_ok(work: [i32; 4], allocs: &[[i32; 4]]) -> Result<(), String> {
    for a in allocs {
        if a[2] <= 0 || a[3] <= 0 {
            return Err("allocation has non-positive area".to_owned());
        }
        if !inside(work, *a) {
            return Err("allocation escapes the work area".to_owned());
        }
    }
    for (i, a) in allocs.iter().enumerate() {
        for b in allocs.iter().skip(i + 1) {
            if overlap(*a, *b) {
                return Err("allocations overlap".to_owned());
            }
        }
    }
    let sum: i64 = allocs.iter().map(|a| area(*a)).sum();
    if sum != area(work) {
        return Err(format!(
            "allocation area {sum} != work area {} (gap)",
            area(work)
        ));
    }
    Ok(())
}

fn fail(step: &str, msg: impl std::fmt::Display) -> String {
    format!("{step}: {msg}")
}

// ---------------------------------------------------------------------------
// Build authored reports (host inputs only, never reply output).
// ---------------------------------------------------------------------------

fn case_mode(mode: &str) -> Result<WorkspaceMode, String> {
    match mode {
        "per-output-local" => Ok(WorkspaceMode::PerOutputLocal),
        other => Err(format!("mode {other:?} is not covered by this narrow case")),
    }
}

/// Case-built command for one step, shared by the report builder and the
/// checker: the executed report must carry exactly this command.
fn build_command(
    step: &CaseStep,
    binding: &BTreeMap<String, String>,
) -> Result<Option<WorldCommand>, String> {
    match &step.command {
        None => Ok(None),
        Some(cmd) => Ok(Some(match cmd.op.as_str() {
            "select-workspace" => WorldCommand::SelectWorkspace {
                output: OutputId(cmd.output.clone().ok_or("select-workspace needs output")?),
                workspace: binding
                    .get(
                        cmd.workspace
                            .as_deref()
                            .ok_or("select-workspace needs workspace")?,
                    )
                    .cloned()
                    .ok_or_else(|| format!("{}: unbound workspace alias", step.name))?,
            },
            "select-previous" => WorldCommand::SelectPrevious {
                output: cmd.output.clone().map(OutputId),
            },
            other => return Err(format!("command {other:?} is not covered")),
        })),
    }
}

fn build_report(
    case: &CaseFile,
    step: &CaseStep,
    mode: WorkspaceMode,
    binding: &BTreeMap<String, String>,
) -> Result<WorldReport, String> {
    if !case.settings.default_tiled {
        return Err("only default_tiled=true is covered".to_owned());
    }
    let screen = &case.screens[0];
    if screen.gap != 0 || screen.outer_gap != 0 || !screen.complete {
        return Err("only zero-gap complete screens are covered".to_owned());
    }
    let mut members = step.expect.members.clone();
    members.sort();
    let mut keys: Vec<&String> = step.facts.keys().collect();
    keys.sort();
    if keys != members.iter().collect::<Vec<_>>() {
        return Err(format!(
            "{}: facts must name exactly the members",
            step.name
        ));
    }
    let resolve = |alias: &str| -> Result<String, String> {
        binding
            .get(alias)
            .cloned()
            .ok_or_else(|| format!("{}: unbound alias {alias:?}", step.name))
    };
    let mut windows = Vec::new();
    for id in &case.windows {
        let Some(rect) = step.facts.get(id) else {
            continue;
        };
        windows.push(WindowFact {
            window: WindowId(id.clone()),
            output: OutputId(screen.out.clone()),
            rect: Some(rect4(*rect)),
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
            native_workspace: None,
        });
    }
    let command = build_command(step, binding)?;
    let mut views = Vec::new();
    for (out, alias) in &step.views {
        views.push(ScreenView {
            output: OutputId(out.clone()),
            view: resolve(alias)?,
        });
    }
    Ok(WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: case.owner.clone(),
        adapter_incarnation: case.adapter.clone(),
        core_generation: case.generation.clone(),
        observation_seq: step.observation,
        request_seq: step.request,
        request_id: format!("req-{}", step.request),
        command_seq: step.command.as_ref().map(|c| c.seq),
        command,
        workspace_mode: mode,
        screens: vec![ScreenReport {
            output: OutputId(screen.out.clone()),
            full_rect: Some(rect4(screen.full)),
            work_area: Some(rect4(screen.work)),
            gap: screen.gap,
            outer_gap: screen.outer_gap,
            complete: screen.complete,
            primary: screen.primary,
        }],
        windows,
        views,
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: case.settings.default_tiled,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: vec![],
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: vec![],
        member_attempts: vec![],
        view_attempts: vec![],
        clear_attempts: vec![],
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
    })
}

/// Bind home/away once from a scratch bootstrap run of the authored step-0
/// report (host input only, no oracle involved). The real checked run binds
/// again from its own first reply and must agree; binding is never rebound.
fn bootstrap_binding(
    case: &CaseFile,
    mode: WorkspaceMode,
) -> Result<BTreeMap<String, String>, String> {
    let step0 = case.steps.first().ok_or("case has no steps")?;
    if case.screens.len() != 1 {
        return Err("narrow case covers exactly one screen".to_owned());
    }
    let mut probe = WorldEngine::new(&case.generation).ok_or("bad generation")?;
    let first = build_report(case, step0, mode, &BTreeMap::new())?;
    let reply = match probe.handle_report(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => return Err(format!("bootstrap probe: expected accepted, got {other:?}")),
    };
    let home = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == case.screens[0].out)
        .and_then(|o| o.logical_view.clone())
        .ok_or("bootstrap has no view")?;
    let mut ring: Vec<(usize, String)> = reply
        .workspaces
        .iter()
        .filter(|w| w.output.0 == case.screens[0].out)
        .map(|w| (w.order, w.id.clone()))
        .collect();
    ring.sort();
    if ring.len() != 2 || ring[0].1 != home {
        return Err("bootstrap ring is not [home, away]".to_owned());
    }
    Ok(BTreeMap::from([
        ("home".to_owned(), home),
        ("away".to_owned(), ring[1].1.clone()),
    ]))
}

fn build_reports(case: &CaseFile) -> Result<Vec<WorldReport>, String> {
    if case.steps.is_empty() {
        return Err("case has no steps".to_owned());
    }
    if case.screens.len() != 1 {
        return Err("narrow case covers exactly one screen".to_owned());
    }
    let mode = case_mode(&case.mode)?;
    let binding = bootstrap_binding(case, mode)?;
    case.steps
        .iter()
        .map(|step| build_report(case, step, mode, &binding))
        .collect()
}

/// Run authored reports through a fresh engine. Only `Accepted` counts:
/// duplicates, consumed commands, and rejections fail loudly (no relabel).
fn run_reports(case: &CaseFile, reports: &[WorldReport]) -> Result<Vec<WorldReply>, String> {
    if reports.len() != case.steps.len() {
        return Err("report count != case steps".to_owned());
    }
    let mut engine = WorldEngine::new(&case.generation).ok_or("bad generation")?;
    let mut replies = Vec::new();
    for (step, report) in case.steps.iter().zip(reports.iter()) {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => replies.push(reply),
            WorldOutcome::Duplicate(_) => {
                return Err(fail(&step.name, "duplicate reply is not fresh acceptance"));
            }
            WorldOutcome::CommandConsumed(_) => {
                return Err(fail(&step.name, "consumed command never re-executes here"));
            }
            WorldOutcome::Rejected { kind, message } => {
                return Err(format!("{}: rejected {kind} {message}", step.name));
            }
        }
    }
    Ok(replies)
}

fn execute(case: &CaseFile) -> Result<Vec<(WorldReport, WorldReply)>, String> {
    let reports = build_reports(case)?;
    let replies = run_reports(case, &reports)?;
    Ok(reports.into_iter().zip(replies).collect())
}

// ---------------------------------------------------------------------------
// Checker: every assertion reads the ACTUAL accepted reply (plus the report
// it was built from for pairing). Expected tiles are a separate additional
// check; coverage runs over actual reply allocations.
// ---------------------------------------------------------------------------

fn check(case: &CaseFile, pairs: &[(WorldReport, WorldReply)]) -> Result<Vec<String>, String> {
    if case.steps.is_empty() {
        return Err("case has no steps".to_owned());
    }
    if case.screens.len() != 1 {
        return Err("narrow case covers exactly one screen".to_owned());
    }
    if pairs.len() != case.steps.len() {
        return Err(format!(
            "checkpoint count {} != case steps {} (truncated or extra)",
            pairs.len(),
            case.steps.len()
        ));
    }
    let screen = &case.screens[0];
    let work = screen.work;
    let mut home = String::new();
    let mut ring_orders: BTreeMap<String, usize> = BTreeMap::new();
    let mut lines = Vec::new();
    let mut prev_request = 0u64;
    let mut prev_observation = 0u64;
    let mut prev_revision = 0u64;
    for (index, (step, (report, reply))) in case.steps.iter().zip(pairs.iter()).enumerate() {
        let at = step.name.as_str();
        // Freshness across the ordered steps (no missing intermediate).
        if index == 0 {
            if step.request != 1 || step.observation != 1 {
                return Err(fail(at, "first step must be req/obs 1"));
            }
        } else if step.request != prev_request + 1 || step.observation != prev_observation + 1 {
            return Err(fail(
                at,
                "non-fresh req/obs chain (missing intermediate observation)",
            ));
        }
        prev_request = step.request;
        prev_observation = step.observation;
        // The actual report binds exactly to its step (ids and counters);
        // a self-consistent renamed report/reply pair still fails here.
        let want_request = format!("req-{}", step.request);
        if report.request_seq != step.request
            || report.observation_seq != step.observation
            || report.request_id != want_request
        {
            return Err(fail(at, "report identity != step"));
        }
        // Report/reply pairing on actual fields.
        if reply.request_id != report.request_id
            || reply.based_on_observation_seq != report.observation_seq
            || reply.adapter_incarnation != report.adapter_incarnation
            || reply.core_generation != report.core_generation
            || reply.contract_version != report.contract_version
        {
            return Err(fail(at, "reply does not pair this exact report"));
        }
        if !reply.ready {
            return Err(fail(at, "reply not ready"));
        }
        if reply.command_consumed {
            return Err(fail(at, "command consumed without execution"));
        }
        if reply.contract_version != WORLD_CONTRACT_VERSION
            || reply.adapter_incarnation != case.adapter
            || reply.core_generation != case.generation
            || report.owner != case.owner
        {
            return Err(fail(at, "report/reply provenance != case identity"));
        }
        if index == 0 {
            if reply.state_revision != 1 {
                return Err(fail(
                    at,
                    format!("first revision {} != 1", reply.state_revision),
                ));
            }
        } else if reply.state_revision != prev_revision + 1 {
            return Err(fail(at, "non-fresh revision chain"));
        }
        prev_revision = reply.state_revision;
        // Command/settlement consistency with the authored report.
        let pending = match step.expect.settlement.as_str() {
            "command-pending" => true,
            "settled" => false,
            _ => return Err(fail(at, "unknown settlement status")),
        };
        if pending != step.command.is_some() {
            return Err(fail(at, "settlement must match command presence"));
        }
        if !pending && !step.expect.transfers.is_empty() {
            return Err(fail(at, "settled expectation carries a transfer"));
        }
        if let Some(cmd) = &step.command {
            if report.command_seq != Some(cmd.seq) {
                return Err(fail(at, "report command_seq != case seq"));
            }
        } else if report.command_seq.is_some() {
            return Err(fail(at, "settled report carries a command_seq"));
        }
        // Bind home/away ONCE from the bootstrap reply ring; freeze orders.
        if index == 0 {
            home = reply
                .outputs
                .iter()
                .find(|o| o.output.0 == screen.out)
                .and_then(|o| o.logical_view.clone())
                .ok_or_else(|| fail(at, "bootstrap output has no view to bind home"))?;
            let mut ring: Vec<(usize, String)> = reply
                .workspaces
                .iter()
                .filter(|w| w.output.0 == screen.out)
                .map(|w| (w.order, w.id.clone()))
                .collect();
            ring.sort();
            if ring.len() != 2 || ring[0].1 != home {
                return Err(fail(at, "bootstrap ring is not [home, away]"));
            }
            ring_orders = ring.into_iter().map(|(o, id)| (id, o)).collect();
        }
        let away = ring_orders
            .keys()
            .find(|id| id.as_str() != home)
            .cloned()
            .ok_or_else(|| fail(at, "away binding lost"))?;
        let resolve = |alias: &str| -> Result<&str, String> {
            match alias {
                "home" => Ok(home.as_str()),
                "away" => Ok(away.as_str()),
                other => Err(format!("unknown workspace alias {other:?}")),
            }
        };
        // The executed report must carry exactly the case-built command.
        {
            let bound: BTreeMap<String, String> = BTreeMap::from([
                ("home".to_owned(), home.clone()),
                ("away".to_owned(), away.clone()),
            ]);
            let expected = build_command(step, &bound).map_err(|e| fail(at, e))?;
            if report.command != expected {
                return Err(fail(at, "report command != case-built command"));
            }
        }
        // Ring set + orders frozen (compared by bound ids, never lexically).
        {
            let mut current: BTreeMap<String, usize> = BTreeMap::new();
            for w in reply.workspaces.iter().filter(|w| w.output.0 == screen.out) {
                if current.insert(w.id.clone(), w.order).is_some() {
                    return Err(fail(at, "duplicate workspace id"));
                }
            }
            if current != ring_orders {
                return Err(fail(at, "ring moved (rebinding is forbidden)"));
            }
            for w in &reply.workspaces {
                if w.tiling_mode.as_str() != "tiled"
                    || !w.existence
                    || w.hold.is_some()
                    || w.scope != screen.out
                    || w.output.0 != screen.out
                    || w.native_binding.is_some()
                {
                    return Err(fail(at, "workspace row carries unsupported class"));
                }
            }
        }
        // Outputs: exactly the case screen showing the expected view.
        if reply.outputs.len() != 1 {
            return Err(fail(at, "outputs must carry exactly the case screen"));
        }
        let out = &reply.outputs[0];
        let want_view = resolve(&step.expect.view).map_err(|e| fail(at, e))?;
        if out.output.0 != screen.out
            || out.logical_view.as_deref() != Some(want_view)
            || out.tiling_mode.as_str() != "tiled"
            || out.hold.is_some()
        {
            return Err(fail(
                at,
                format!("output does not show expected view {:?}", step.expect.view),
            ));
        }
        // Scopes: exact expected set, every listed scope controlled.
        {
            let home_scope = format!("{}/{home}", screen.out);
            let away_scope = format!("{}/{away}", screen.out);
            let mut scopes: Vec<&str> = reply.scopes.iter().map(|s| s.scope.as_str()).collect();
            scopes.sort();
            let mut want = if index == 0 {
                vec![home_scope.as_str()]
            } else {
                vec![away_scope.as_str(), home_scope.as_str()]
            };
            want.sort();
            if scopes != want {
                return Err(fail(at, format!("scopes {scopes:?} != {want:?}")));
            }
            for s in &reply.scopes {
                if s.authority.as_str() != "controlled" {
                    return Err(fail(at, format!("scope {} is not controlled", s.scope)));
                }
            }
        }
        // Desired roster: exact member set. Per row: homed home, actual
        // allocation present (coverage runs over the actuals below),
        // oracle tile equality, separate effective check, authorities.
        let mut members = step.expect.members.clone();
        members.sort();
        let mut got: Vec<String> = reply.windows.iter().map(|w| w.lifetime.0.clone()).collect();
        got.sort();
        if got != members {
            return Err(fail(
                at,
                format!("desired roster {got:?} != members {members:?}"),
            ));
        }
        let mut allocs: Vec<[i32; 4]> = Vec::new();
        for row in &reply.windows {
            let tile = step
                .expect
                .tiles
                .get(&row.lifetime.0)
                .ok_or_else(|| fail(at, format!("no authored tile for {}", row.lifetime.0)))?;
            let alloc = row.allocation.map(rect_of).ok_or_else(|| {
                fail(
                    at,
                    format!("{} has no allocation (held/fenced?)", row.lifetime.0),
                )
            })?;
            allocs.push(alloc);
            if row.output.0 != screen.out || row.workspace != home {
                return Err(fail(at, format!("{} homed off home", row.lifetime.0)));
            }
            if alloc != *tile {
                return Err(fail(
                    at,
                    format!("{} allocation != authored tile", row.lifetime.0),
                ));
            }
            // Effective frame: separate check, equal to allocation here.
            let eff = row
                .effective_frame
                .map(rect_of)
                .ok_or_else(|| fail(at, format!("{} has no effective frame", row.lifetime.0)))?;
            if eff != *tile {
                return Err(fail(
                    at,
                    format!("{} effective != authored tile", row.lifetime.0),
                ));
            }
            if row.frame_authority.as_str() != "controlled"
                || row.frame_authority.hold_reason().is_some()
                || row.membership_authority.as_str() != "controlled"
                || row.membership_authority.hold_reason().is_some()
                || row.floating
                || row.float_origin.is_some()
                || row.frame_operation.is_some()
                || row.clear_operation.is_some()
            {
                return Err(fail(
                    at,
                    format!("{} carries unsupported class", row.lifetime.0),
                ));
            }
        }
        coverage_ok(work, &allocs).map_err(|e| fail(at, e))?;
        // Transfers: settled steps carry none; command steps pin the exact
        // JSON flight, linked to the command sequence.
        if pending {
            let cmd = step.command.as_ref().expect("pending implies command");
            let mut want = Vec::new();
            for t in &step.expect.transfers {
                let ws = match &t.workspace {
                    Some(alias) => resolve(alias).map_err(|e| fail(at, e))?.to_owned(),
                    None => return Err(fail(at, "transfer workspace must name an alias")),
                };
                want.push((
                    t.op.clone(),
                    t.kind.clone(),
                    t.window.clone(),
                    t.output.clone(),
                    ws,
                    t.follow,
                    t.held,
                ));
            }
            if want.len() != 1 || reply.transfers.len() != 1 {
                return Err(fail(at, "command step must pin exactly one flight"));
            }
            let (t, w) = (&reply.transfers[0], &want[0]);
            let kind = match &t.kind {
                tiler_core::world_report::TransferKind::View => "view",
                _ => "other",
            };
            if t.operation_id != w.0
                || kind != w.1
                || t.target_output.0 != w.3
                || t.target_workspace != w.4
                || t.follow != w.5
                || t.held != w.6
                || w.0 != format!("op-{}", cmd.seq)
                || w.1 != "view"
                || w.5
                || w.6
            {
                return Err(fail(at, "transfer != pinned unheld view flight"));
            }
            if w.2.is_some() {
                return Err(fail(at, "view flight carries no window"));
            }
        } else if !reply.transfers.is_empty() {
            return Err(fail(at, "settled step carries a pending transfer"));
        }
        // Effects: exact ordered set. Narrow reply classes: all absent.
        // Source semantics (world_model.rs, world_commands.rs): one-shot
        // effects mint issued_revision = state_revision + 1, which finish()
        // publishes as this reply.state_revision.
        {
            let want: Vec<(String, String, String)> = step
                .expect
                .effects
                .iter()
                .map(|e| (e.id.clone(), e.kind.clone(), e.target.clone()))
                .collect();
            let got: Vec<(String, String, String)> = reply
                .effects
                .iter()
                .map(|e| (e.id.clone(), e.kind.as_str().to_owned(), e.target.0.clone()))
                .collect();
            if got != want {
                return Err(fail(at, "effects != expected one-shot set"));
            }
            for e in &reply.effects {
                if e.issued_revision != reply.state_revision {
                    return Err(fail(at, "effect issued_revision != reply revision"));
                }
            }
            if reply.command_result.is_some()
                || reply.gesture_settled.is_some()
                || !reply.operation_settled.is_empty()
                || reply.cursor_warp.is_some()
            {
                return Err(fail(at, "unsupported reply class"));
            }
        }
        lines.push(format!(
            "ok {} req={} rev={} obs={} view={} members={} {}",
            step.name,
            report.request_id,
            reply.state_revision,
            report.observation_seq,
            step.expect.view,
            members.len(),
            step.expect.settlement,
        ));
    }
    Ok(lines)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, case] = args.as_slice() else {
        eprintln!("usage: world_acceptance CASE");
        std::process::exit(2);
    };
    match run_case(case) {
        Ok((name, lines)) => {
            for line in &lines {
                println!("{line}");
            }
            println!("ACCEPT {name}: {}/{} checkpoints", lines.len(), lines.len());
        }
        Err(error) => {
            eprintln!("REJECT: {error}");
            std::process::exit(1);
        }
    }
}

fn run_case(path: &str) -> Result<(String, Vec<String>), String> {
    let case = load_case(path)?;
    let pairs = execute(&case)?;
    let lines = check(&case, &pairs)?;
    Ok((case.name.clone(), lines))
}

#[cfg(test)]
mod acceptance_tests {
    use super::*;
    use tiler_core::world_report::{HoldReason, ScopeAuthority};

    fn case_path() -> String {
        format!(
            "{}/../../test-fixtures/world-actions.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn case_json() -> serde_json::Value {
        let text = std::fs::read_to_string(case_path()).expect("read fixture");
        serde_json::from_str(&text).expect("parse fixture")
    }

    fn case_file(value: &serde_json::Value) -> CaseFile {
        serde_json::from_value(value.clone()).expect("case schema")
    }

    fn base_pairs() -> (CaseFile, Vec<(WorldReport, WorldReply)>) {
        let case = load_case(&case_path()).expect("load fixture");
        let pairs = execute(&case).expect("engine run");
        (case, pairs)
    }

    fn must_fail(case: &CaseFile, pairs: &[(WorldReport, WorldReply)], needle: &str) {
        let error = check(case, pairs).expect_err("must reject");
        assert!(error.contains(needle), "error {error:?} misses {needle:?}");
    }

    fn step_of<'v>(value: &'v mut serde_json::Value, name: &str) -> &'v mut serde_json::Value {
        value["steps"]
            .as_array_mut()
            .expect("steps")
            .iter_mut()
            .find(|s| s["name"] == name)
            .expect("step")
    }

    #[test]
    fn accepts_fixture() {
        let (case, pairs) = base_pairs();
        let lines = check(&case, &pairs).expect("accept");
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn settled_rejects_lingering_flight() {
        // Drop the away logical receipt: the select-away flight stays
        // outstanding into away-settled, which must fail as settled.
        let mut value = case_json();
        step_of(&mut value, "away-settled")["views"] = serde_json::json!({});
        let case = case_file(&value);
        let pairs = execute(&case).expect("engine accepts with outstanding flight");
        must_fail(&case, &pairs, "pending transfer");
    }

    #[test]
    fn pending_needs_explicit_command_settlement() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "select-away")["expect"]["settlement"] = serde_json::json!("settled");
        must_fail(&case_file(&value), &pairs, "must match command presence");
    }

    #[test]
    fn unknown_settlement_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "away-settled")["expect"]["settlement"] = serde_json::json!("unknown");
        must_fail(&case_file(&value), &pairs, "unknown settlement");
    }

    #[test]
    fn settled_expectation_cannot_carry_transfer() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        let transfer = step_of(&mut value, "select-away")["expect"]["transfers"].clone();
        step_of(&mut value, "away-settled")["expect"]["transfers"] = transfer;
        must_fail(
            &case_file(&value),
            &pairs,
            "settled expectation carries a transfer",
        );
    }

    #[test]
    fn truncated_case_rejected() {
        let mut value = case_json();
        value["steps"].as_array_mut().expect("steps").pop();
        must_fail(&case_file(&value), &base_pairs().1, "truncated or extra");
    }

    #[test]
    fn empty_steps_rejected() {
        let mut value = case_json();
        value["steps"] = serde_json::json!([]);
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("no steps"), "unexpected: {error}");
    }

    #[test]
    fn wrong_screen_count_rejected() {
        let mut value = case_json();
        value["screens"] = serde_json::json!([]);
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("exactly one screen"), "unexpected: {error}");
        let mut value = case_json();
        let extra = value["screens"][0].clone();
        value["screens"]
            .as_array_mut()
            .expect("screens")
            .push(extra);
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("exactly one screen"), "unexpected: {error}");
    }

    #[test]
    fn swapped_checkpoints_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        let steps = value["steps"].as_array_mut().expect("steps");
        steps.swap(3, 4);
        must_fail(&case_file(&value), &pairs, "non-fresh");
    }

    #[test]
    fn stale_observation_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "away-settled")["observation"] = serde_json::json!(3);
        must_fail(&case_file(&value), &pairs, "non-fresh");
    }

    #[test]
    fn wrong_expected_view_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "home-settled")["expect"]["view"] = serde_json::json!("away");
        must_fail(&case_file(&value), &pairs, "expected view");
    }

    #[test]
    fn oracle_roster_mismatch_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "home-settled")["expect"]["members"] =
            serde_json::json!(["win-a", "win-b"]);
        must_fail(&case_file(&value), &pairs, "roster");
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "home-settled")["expect"]["tiles"]["win-a"] =
            serde_json::json!([400, 0, 399, 280]);
        must_fail(&case_file(&value), &pairs, "authored tile");
    }

    #[test]
    fn oracle_flight_mismatch_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "select-away")["expect"]["transfers"][0]["op"] =
            serde_json::json!("op-99");
        must_fail(&case_file(&value), &pairs, "pinned");
        let (mut value, pairs) = (case_json(), base_pairs().1);
        step_of(&mut value, "select-away")["expect"]["transfers"] = serde_json::json!([]);
        must_fail(&case_file(&value), &pairs, "exactly one flight");
    }

    #[test]
    fn wrong_pairing_owner_rejected() {
        let (mut value, pairs) = (case_json(), base_pairs().1);
        value["owner"] = serde_json::json!("owner-zzz");
        must_fail(&case_file(&value), &pairs, "provenance");
    }

    #[test]
    fn renamed_report_reply_pair_rejected() {
        // Rename consistently on both sides of the pair: pairing still
        // holds, but the pair no longer matches its step.
        let (case, mut pairs) = base_pairs();
        pairs[2].0.request_seq = 99;
        pairs[2].0.request_id = "req-99".to_owned();
        pairs[2].0.observation_seq = 99;
        pairs[2].1.request_id = "req-99".to_owned();
        pairs[2].1.based_on_observation_seq = 99;
        must_fail(&case, &pairs, "report identity != step");
    }

    #[test]
    fn report_command_must_match_case() {
        let (case, mut pairs) = base_pairs();
        pairs[2].0.command = Some(WorldCommand::SelectPrevious { output: None });
        must_fail(&case, &pairs, "report command != case-built command");
    }

    #[test]
    fn effect_revision_must_match_reply() {
        let (case, mut pairs) = base_pairs();
        pairs[1].1.effects[0].issued_revision = 1;
        must_fail(&case, &pairs, "issued_revision");
    }

    #[test]
    fn mutated_revision_rejected() {
        let (case, mut pairs) = base_pairs();
        pairs[5].1.state_revision = 5;
        must_fail(&case, &pairs, "revision chain");
    }

    #[test]
    fn corrupted_reply_shape_rejected() {
        // Mutations of actual engine output: the checker must reject every
        // corrupted reply shape (fail-closed), never repair it.
        let (case, mut pairs) = base_pairs();
        pairs[5].1.windows[0].allocation = None;
        must_fail(&case, &pairs, "no allocation");
        let (case, mut pairs) = base_pairs();
        pairs[5].1.windows[0].effective_frame = None;
        must_fail(&case, &pairs, "no effective frame");
        let (case, mut pairs) = base_pairs();
        let row = pairs[5]
            .1
            .windows
            .iter_mut()
            .find(|w| w.lifetime.0 == "win-a")
            .expect("win-a row");
        row.effective_frame = Some(rect4([0, 0, 8, 8]));
        must_fail(&case, &pairs, "effective != authored");
        let (case, mut pairs) = base_pairs();
        pairs[5].1.scopes[0].authority = ScopeAuthority::Hold(HoldReason::Unreadable);
        must_fail(&case, &pairs, "not controlled");
    }

    fn mutated_reports(case: &CaseFile) -> Vec<WorldReport> {
        let mode = case_mode(&case.mode).expect("mode");
        let binding = bootstrap_binding(&case, mode).expect("bind");
        case.steps
            .iter()
            .map(|s| build_report(&case, s, mode, &binding).expect("build"))
            .collect()
    }

    #[test]
    fn unreadable_fact_holds_scope() {
        let case = load_case(&case_path()).expect("load fixture");
        let mut reports = mutated_reports(&case);
        reports[0].windows[0].rect = None;
        let replies = run_reports(&case, &reports).expect("engine accepts the hold");
        let pairs: Vec<(WorldReport, WorldReply)> = reports.into_iter().zip(replies).collect();
        must_fail(&case, &pairs, "not controlled");
    }

    #[test]
    fn incomplete_screen_fences_bootstrap() {
        let case = load_case(&case_path()).expect("load fixture");
        let mut reports = mutated_reports(&case);
        reports[0].screens[0].complete = false;
        let replies = run_reports(&case, &reports).expect("engine accepts the fence");
        let pairs: Vec<(WorldReport, WorldReply)> = reports.into_iter().zip(replies).collect();
        let error = check(&case, &pairs).expect_err("must reject");
        assert!(error.contains("ring"), "unexpected: {error}");
    }

    #[test]
    fn coverage_rejects_bad_tilings() {
        let work = [0, 0, 800, 560];
        assert!(coverage_ok(work, &[[0, 0, 400, 560], [400, 0, 400, 560]]).is_ok());
        assert!(coverage_ok(work, &[[0, 0, 399, 560], [400, 0, 400, 560]]).is_err());
        assert!(coverage_ok(work, &[[0, 0, 401, 560], [400, 0, 400, 560]]).is_err());
        assert!(coverage_ok(work, &[[0, 0, 400, 560], [400, 0, 400, 0]]).is_err());
        assert!(coverage_ok(work, &[[0, 0, 400, 560], [800, 0, 400, 560]]).is_err());
    }

    #[test]
    fn unsupported_case_rejected() {
        let mut value = case_json();
        value["screens"][0]["gap"] = serde_json::json!(8);
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("zero-gap"), "unexpected: {error}");
        let mut value = case_json();
        step_of(&mut value, "select-away")["command"]["op"] = serde_json::json!("send");
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("not covered"), "unexpected: {error}");
        let mut value = case_json();
        value["mode"] = serde_json::json!("shared");
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("not covered"), "unexpected: {error}");
    }

    #[test]
    fn facts_must_name_members() {
        let mut value = case_json();
        step_of(&mut value, "bootstrap")["facts"]
            .as_object_mut()
            .expect("facts")
            .remove("win-b");
        let error = execute(&case_file(&value)).expect_err("must reject");
        assert!(error.contains("facts must name"), "unexpected: {error}");
    }
}
