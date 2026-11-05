//! Test-only genuine internal-v4 stress twin (unit 6 sub-increment 7f-5).
//!
//! Runs the authored duo/trio cases over all three simulated-host schedules
//! through the real internal v4 boundary
//! ([`WorldEngine::handle_report_v4_for_test`]), with no legacy
//! [`WorldReport`](tiler_core::world_report::WorldReport) conversion, no
//! logical [`ViewAttempt`](tiler_core::world_report::ViewAttempt)
//! fabrication (that lane is native-only and refused without the host-owned
//! capability), and no changes to the legacy public stress path, fixture
//! files, schedules, report counts, or oracles. Selector-attempt ingress is absent
//! from every v4 report: native-free selection completes through the frozen
//! binding plus the blocked-`LastWrite` verdict lane, and the driver-side
//! predicted alias only steers simulation bookkeeping, never wire ingress.
//! The v4 oracle reads genuine [`WorldReplyV4`](tiler_core::world_report::WorldReplyV4)
//! rows directly, never a converted legacy reply.

use super::{
    Pending, PendingMember, RawAction, StressCase, StressSchedule, StressStats, bind_engine,
    build_command, case_mode, coverage_ok, fail, load_stress_case, member_proof, rect_of, rect4,
    resolve,
};
use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    AuthorityScope, LastWrite, LastWritePath, LastWriteProperty, OutputIdentity,
    PresentationIngressV4, PresentationRowV4, ScopeAuthority, ScreenReport,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef, WorldCommand, WorldReplyV4, WorldReportV4,
};

struct SimV4 {
    req: u64,
    obs: u64,
    facts: BTreeMap<String, [i32; 4]>,
    live: BTreeSet<String>,
    pending: Pending,
    /// Truthful raw membership proofs staged for the next receipt report.
    next_attempts: Vec<tiler_core::world_report::MemberAttempt>,
    /// Sends accepted since the last observation report (in-flight guard).
    outstanding: BTreeSet<String>,
    home: String,
    /// Current away-slot workspace id. Home never empties, but the away
    /// slot rebinds when lifecycle prunes an emptied workspace and keeps
    /// the trailing spare. Hand-authored expectations stay in alias space;
    /// ids resolve late from this slot.
    away: String,
    /// Every workspace id ever seen in a reply ring. Stale scope rows for
    /// pruned workspaces must name a seen id, never an invented one.
    seen: BTreeSet<String>,
    out: String,
    /// Predicted logical slot ("home"/"away") from commands issued since
    /// the last reply. Driver-side only: steers command resolution and
    /// foreground prediction, never wire ingress on the v4 lane.
    predicted_alias: String,
    /// Predicted real foreground: the managed window the adapter observes,
    /// or `None` when no managed fixture window can hold it. Retained core
    /// focus (`focus` below) stays untouched: hiding never clears it.
    foreground: Option<String>,
    /// Commanded homing truth (window id -> home/away alias), updated at
    /// command/admit time so `focus-window` validation is schedule-stable.
    homing_alias: BTreeMap<String, String>,
    /// Last non-empty one-shot effect target (window id, "" when none yet).
    focus: String,
    /// Recorded desire at issue (pending, not proof): per-window commanded
    /// target and issuing revision from the actual issuing reply's own
    /// visibility rows. Blocked-only lane, identical to the legacy driver:
    /// sampled as a `NotCalled`/`ScopeHeld` no-call on the next observation
    /// receipt, then spent without retry/upgrade.
    grants: BTreeMap<String, (bool, u64)>,
    /// Accepted (report, reply) pairs in emission order.
    pairs: Vec<(WorldReportV4, WorldReplyV4)>,
}

fn build_report_v4(
    case: &StressCase,
    _mode: WorkspaceMode,
    sim: &SimV4,
    command: Option<WorldCommand>,
    command_seq: Option<u64>,
    closed: Vec<String>,
) -> Result<WorldReportV4, String> {
    let screen = &case.screens[0];
    let mut live: Vec<String> = sim.live.iter().cloned().collect();
    live.sort();
    let mut keys: Vec<&String> = sim.facts.keys().collect();
    keys.sort();
    let live_refs: Vec<&String> = live.iter().collect();
    if keys != live_refs {
        return Err("facts must name exactly the live roster".to_owned());
    }
    let mut windows = Vec::new();
    for id in &live {
        // The raw visibility facts the post sample must agree with: the
        // sample below reuses this exact value, never a desired state.
        let visibility = tiler_core::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        };
        windows.push(WindowFact {
            window: WindowId(id.clone()),
            output: Some(OutputId(sim.out.clone())),
            rect: sim.facts.get(id).copied().map(rect4),
            fullscreen: Some(false),
            maximized: Some(false),
            sticky: Some(false),
            minimized: Some(false),
            hints: Some(tiler_core::size_hints::WindowSizeHints::none()),
            native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
            host_hidden: Some(false),
            privileged: Some(false),
            managed: Some(true),
            accepts_input: None,
            activation_inhibited: Some(false),
            tool_window: Some(false),
            owned: Some(false),
            transient: Some(false),
            modal: Some(false),
            resizable: None,
            above: None,
            below: None,
            owner: None,
            parent: None,
            role: None,
            app_id: None,
            executable: None,
            process_id: None,
            class: Some("StressWindow".to_owned()),
            title: None,
            window_type: Some(tiler_core::world_report::WindowType::Normal),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
            layer: Some(0),
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility,
            // Correlated visibility verdicts ride the next observation
            // report only: the recorded target/revision plus the actual
            // read sampled now (post at this report's observation, post
            // state from the raw facts above). This lane holds every
            // write, so the verdict is always the blocked no-call shape,
            // never an invented write: a hide target cannot read back
            // hidden while the raw fact stays visible, so it fails
            // terminally through the common path. Command reports carry
            // none (same-report raw would guess a future issuing
            // revision).
            last_write: if command.is_none() {
                sim.grants.get(id).map(|(shown, rev)| LastWrite {
                    property: LastWriteProperty::Visibility,
                    target: *shown,
                    revision: *rev,
                    path: LastWritePath::Staged,
                    setter: tiler_core::world_report::VisibilitySetter::NotCalled,
                    reason: tiler_core::world_report::VisibilityReason::ScopeHeld,
                    pre: None,
                    post: Some(tiler_core::world_report::VisibilitySample {
                        obs: sim.obs,
                        lifetime_matched: Some(true),
                        visibility,
                        minimized: Some(false),
                        host_hidden: Some(false),
                    }),
                })
            } else {
                None
            },
        });
    }
    // No view ingress on the v4 lane: no `views` echo exists on
    // [`WorldReportV4`], and `view_attempts` stays empty (native-only lane,
    // refused without the host-owned capability). Native-free selection
    // completes through the frozen binding plus the verdict lane above.
    Ok(WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: case.adapter.clone(),
        core_generation: case.generation.clone(),
        observation_seq: sim.obs,
        request_seq: sim.req,
        request_id: format!("req-{}", sim.req),
        command_seq,
        command,
        screens: vec![ScreenReport {
            output: OutputId(screen.out.clone()),
            full_rect: Some(rect4(screen.full)),
            work_area: Some(rect4(screen.work)),
            complete: screen.complete,
            scale: 1.0,
            identity: OutputIdentity::unknown(),
        }],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: closed.into_iter().map(WindowId).collect(),
        native: vec![],
        held_native: vec![],
        cursor: None,
        cursor_hit: None,
        // Truthful observation: the predicted real foreground, "" when no
        // managed window can hold it. Never the command subject.
        focused_global: sim.foreground.clone().unwrap_or_default(),
        action_foreground: None,
        action_cached: None,
        action_outputs: vec![],
        primary_output: if screen.primary {
            Some(OutputId(screen.out.clone()))
        } else {
            None
        },
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: vec![],
        member_attempts: sim.next_attempts.clone(),
        view_attempts: vec![],
        clear_attempts: vec![],
        creation_attempts: vec![],
        held_attempts: vec![],
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),

        intent_records: Vec::new(),
        renderer_input: None,
    })
}

fn bootstrap_binding_v4(
    case: &StressCase,
    mode: WorkspaceMode,
) -> Result<BTreeMap<String, String>, String> {
    let mut probe = WorldEngine::new(&case.generation).ok_or("bad generation")?;
    bind_engine(&mut probe, case, mode)?;
    let sim = SimV4 {
        req: 1,
        obs: 1,
        facts: case.initial_facts.clone(),
        live: case.initial_facts.keys().cloned().collect(),
        pending: Pending::empty(),
        next_attempts: Vec::new(),
        outstanding: BTreeSet::new(),
        home: String::new(),
        away: String::new(),
        seen: BTreeSet::new(),
        out: case.screens[0].out.clone(),
        predicted_alias: String::new(),
        foreground: None,
        homing_alias: case
            .initial_facts
            .keys()
            .map(|w| (w.clone(), "home".to_owned()))
            .collect(),
        focus: String::new(),
        grants: BTreeMap::new(),
        pairs: Vec::new(),
    };
    let first = build_report_v4(case, mode, &sim, None, None, vec![])?;
    let reply = probe
        .handle_report_v4_for_test(&first)
        .map_err(|(kind, message)| format!("bootstrap probe: refused {kind} {message}"))?;
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

fn binding_of_v4(sim: &SimV4) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("home".to_owned(), sim.home.clone()),
        ("away".to_owned(), sim.away.clone()),
    ])
}

/// Refresh the away slot from the latest accepted reply: order 0 stays
/// the frozen home, order 1 is the current away slot (rebinds when
/// lifecycle prunes an emptied workspace).
fn refresh_away_slot_v4(sim: &mut SimV4, at: &str) -> Result<(), String> {
    let (_, reply) = sim.pairs.last().expect("just pushed");
    let mut ordered: Vec<(usize, String)> = reply
        .workspaces
        .iter()
        .filter(|w| w.output.0 == sim.out)
        .map(|w| (w.order, w.id.clone()))
        .collect();
    ordered.sort();
    if ordered.is_empty() || ordered[0].1 != sim.home {
        return Err(fail(at, "home slot moved"));
    }
    if ordered.len() < 2 {
        return Err(fail(at, "away slot missing"));
    }
    for (_, id) in &ordered {
        sim.seen.insert(id.clone());
    }
    sim.away = ordered[1].1.clone();
    Ok(())
}

/// Converge simulated host facts to the actual accepted allocations, exactly
/// like a perfect host applying every desired frame. The oracle literals
/// stay hand-authored; this only keeps later facts readable.
fn converge_facts_v4(sim: &mut SimV4, reply: &WorldReplyV4) {
    for row in &reply.windows {
        if let Some(alloc) = row.allocation {
            sim.facts.insert(row.lifetime.0.clone(), rect_of(alloc));
        }
    }
}

/// Recompute the predicted real foreground after applied proofs: a
/// foreground window whose commanded homing no longer names the commanded
/// shown view is hidden, so no managed window can hold the foreground.
/// Commanded (not settled) truth keeps every schedule identical; flushes
/// settle pending proofs before any observation that reads this.
fn recompute_foreground_v4(sim: &mut SimV4) {
    if let Some(window) = sim.foreground.clone() {
        let shown = sim.homing_alias.get(&window).cloned().unwrap_or_default();
        if shown != sim.predicted_alias {
            sim.foreground = None;
        }
    }
}

fn note_effects_v4(sim: &mut SimV4, reply: &WorldReplyV4, at: &str) -> Result<(), String> {
    for effect in &reply.effects {
        if effect.issued_revision != reply.state_revision {
            return Err(fail(at, "effect issued_revision != reply revision"));
        }
    }
    if let Some(last) = reply.effects.last() {
        sim.focus = last.target.0.clone();
    }
    if reply.command_result.is_some()
        || reply.gesture_settled.is_some()
        || !reply.operation_settled.is_empty()
        || reply.cursor_warp.is_some()
    {
        return Err(fail(at, "unsupported reply class"));
    }
    Ok(())
}

fn check_pairing_v4(
    case: &StressCase,
    report: &WorldReportV4,
    reply: &WorldReplyV4,
    at: &str,
    revision: u64,
) -> Result<(), String> {
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
    if reply.contract_version != WORLD_CONTRACT_VERSION_V4
        || reply.adapter_incarnation != case.adapter
        || reply.core_generation != case.generation
    {
        return Err(fail(at, "report/reply provenance != case identity"));
    }
    if reply.state_revision != revision {
        return Err(fail(
            at,
            format!(
                "revision {} != report count {revision}",
                reply.state_revision
            ),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn check_oracle_v4(
    case: &StressCase,
    sim: &SimV4,
    reply: &WorldReplyV4,
    at: &str,
    view_alias: &str,
    members: &[String],
    homing: &BTreeMap<String, String>,
    tiles: Option<&BTreeMap<String, [i32; 4]>>,
    focus: &str,
    foreground: &Option<String>,
    pager_current: &str,
    pager_occupied: &[String],
    _both_scopes: bool,
) -> Result<(), String> {
    let screen = &case.screens[0];
    let view = resolve(&binding_of_v4(sim), view_alias, at)?;
    let current = resolve(&binding_of_v4(sim), pager_current, at)?;
    // Outputs: exactly the case screen showing the expected view.
    if reply.outputs.len() != 1 {
        return Err(fail(at, "outputs must carry exactly the case screen"));
    }
    let out = &reply.outputs[0];
    if out.output.0 != screen.out
        || out.logical_view.as_deref() != Some(view.as_str())
        || out.tiling_mode.as_str() != "tiled"
        || out.hold.is_some()
    {
        return Err(fail(
            at,
            format!("output does not show expected view {view_alias:?}"),
        ));
    }
    // Ring: home frozen at order 0 (it never empties); the away slot at
    // order 1 rebinds when lifecycle prunes an emptied workspace and keeps
    // the trailing spare. Anything past order 1 is an empty trailing
    // spare: never current, never occupied, never homed.
    let trailing: Vec<(usize, String)>;
    {
        let mut current_ring: BTreeMap<String, usize> = BTreeMap::new();
        for w in reply.workspaces.iter().filter(|w| w.output.0 == screen.out) {
            if current_ring.insert(w.id.clone(), w.order).is_some() {
                return Err(fail(at, "duplicate workspace id"));
            }
        }
        let mut ordered: Vec<(usize, String)> = current_ring
            .iter()
            .map(|(id, order)| (*order, id.clone()))
            .collect();
        ordered.sort();
        if ordered.is_empty() || ordered[0].1 != sim.home || ordered[0].0 != 0 {
            return Err(fail(at, format!("home slot moved: got {current_ring:?}")));
        }
        if ordered.len() < 2 || ordered[1].0 != 1 {
            return Err(fail(at, format!("away slot missing: got {current_ring:?}")));
        }
        trailing = ordered[2..].to_vec();
        for (order, id) in &trailing {
            if *order < 2 {
                return Err(fail(at, "trailing spare precedes the away slot"));
            }
            let _ = id;
        }
        for w in &reply.workspaces {
            if w.id == sim.home {
                continue;
            }
            if trailing.iter().any(|(_, id)| id == &w.id)
                && (w.tiling_mode.as_str() != "tiled"
                    || !w.existence
                    || w.hold.is_some()
                    || w.scope != AuthorityScope::Output(OutputId(screen.out.clone()))
                    || w.output.0 != screen.out
                    || w.native_binding.is_some())
            {
                return Err(fail(at, "trailing spare carries unsupported class"));
            }
        }
    }
    // Uniform authority across this output projects to one output scope.
    {
        let mut scopes: Vec<&AuthorityScope> = reply.scopes.iter().map(|s| &s.scope).collect();
        scopes.sort();
        let output_scope = AuthorityScope::Output(OutputId(screen.out.clone()));
        if scopes != [&output_scope] {
            return Err(fail(at, format!("scopes {scopes:?} != [{output_scope:?}]")));
        }
        for s in &reply.scopes {
            if s.authority.as_str() != "controlled" {
                return Err(fail(at, format!("scope {:?} is not controlled", s.scope)));
            }
        }
        if reply
            .workspaces
            .iter()
            .any(|workspace| workspace.output.0 == screen.out && workspace.scope != output_scope)
        {
            return Err(fail(
                at,
                "workspace row scope does not match output authority",
            ));
        }
    }
    // Roster, homing, allocations, oracle tiles, coverage over actuals.
    let mut want_members = members.to_vec();
    want_members.sort();
    let mut got: Vec<String> = reply.windows.iter().map(|w| w.lifetime.0.clone()).collect();
    got.sort();
    if got != want_members {
        return Err(fail(at, format!("roster {got:?} != {want_members:?}")));
    }
    let mut allocs = Vec::new();
    let mut per_workspace: BTreeMap<String, Vec<[i32; 4]>> = BTreeMap::new();
    for row in &reply.windows {
        let alias = homing
            .get(&row.lifetime.0)
            .ok_or_else(|| fail(at, format!("no homing for {}", row.lifetime.0)))?;
        let ws = resolve(&binding_of_v4(sim), alias, at)?;
        if row.output.0 != screen.out || row.workspace != ws {
            return Err(fail(at, format!("{} homed off {alias:?}", row.lifetime.0)));
        }
        let alloc = row
            .allocation
            .map(rect_of)
            .ok_or_else(|| fail(at, format!("{} has no allocation", row.lifetime.0)))?;
        allocs.push(alloc);
        per_workspace.entry(ws.clone()).or_default().push(alloc);
        let eff = row
            .effective_frame
            .map(rect_of)
            .ok_or_else(|| fail(at, format!("{} has no effective frame", row.lifetime.0)))?;
        if eff != alloc {
            return Err(fail(
                at,
                format!("{} effective != allocation", row.lifetime.0),
            ));
        }
        if let Some(oracle) = tiles {
            let tile = oracle
                .get(&row.lifetime.0)
                .ok_or_else(|| fail(at, format!("no authored tile for {}", row.lifetime.0)))?;
            if alloc != *tile {
                let mut dump: Vec<String> = reply
                    .windows
                    .iter()
                    .map(|r| format!("{}={:?}", r.lifetime.0, r.allocation.map(rect_of)))
                    .collect();
                dump.sort();
                return Err(fail(
                    at,
                    format!(
                        "{} allocation {alloc:?} != authored tile {tile:?}; all=[{}]",
                        row.lifetime.0,
                        dump.join(",")
                    ),
                ));
            }
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
    // Coverage runs over the ACTUAL reply allocations, per homed
    // workspace: each occupied workspace tiles the work area on its own.
    for (ws, group) in &per_workspace {
        coverage_ok(screen.work, group).map_err(|e| fail(at, format!("workspace {ws}: {e}")))?;
    }
    let _ = allocs;
    // Settled checkpoints carry no transfers. The native-free route retires
    // the view parent through frozen selection completion with no refill
    // child, so no echo proof is needed for this invariant.
    if !reply.transfers.is_empty() {
        return Err(fail(at, "checkpoint carries a pending transfer"));
    }
    // Tracked one-shot focus (retained while its workspace hides).
    if sim.focus != focus {
        return Err(fail(
            at,
            format!("tracked focus {:?} != {:?}", sim.focus, focus),
        ));
    }
    // Expected real foreground: null exactly when no managed window can
    // hold it (a hidden focus never counts as actual foreground).
    if sim.foreground.as_ref() != foreground.as_ref() {
        return Err(fail(
            at,
            format!(
                "predicted foreground {:?} != {:?}",
                sim.foreground, foreground
            ),
        ));
    }
    // Pager: ring order, current/occupied flags, labels and ordinals. With
    // the disabled core-only ingress the paint lists stay empty; the native
    // runner compares the actual renderer payload separately.
    {
        let pager = reply
            .presentation
            .pager
            .iter()
            .find(|s| s.output == screen.out)
            .ok_or_else(|| fail(at, "pager has no screen"))?;
        if pager.workspaces.len() != 2 + trailing.len() {
            return Err(fail(at, "pager workspace count != ring"));
        }
        for (index, ws) in pager.workspaces.iter().enumerate() {
            let expected_id = match index {
                0 => sim.home.clone(),
                1 => sim.away.clone(),
                _ => trailing[index - 2].1.clone(),
            };
            if ws.id != expected_id {
                return Err(fail(at, "pager order != ring order"));
            }
            if ws.ordinal != index as u32 + 1 {
                return Err(fail(at, "pager ordinal != 1-based ring position"));
            }
            if ws.label != format!("Workspace {}", ws.ordinal) {
                return Err(fail(at, "pager label != ordinal label"));
            }
            if index < 2 {
                let alias = if ws.id == sim.home { "home" } else { "away" };
                let want_current = ws.id == current;
                if ws.current != want_current {
                    let dump: Vec<String> = pager
                        .workspaces
                        .iter()
                        .map(|w| {
                            format!(
                                "{}:ord{} cur={} occ={}",
                                w.id, w.ordinal, w.current, w.occupied
                            )
                        })
                        .collect();
                    return Err(fail(
                        at,
                        format!(
                            "pager current mark is wrong for {} (id {}): got current={} want {want_current} (current id {current}) pager=[{}]",
                            alias,
                            ws.id,
                            ws.current,
                            dump.join(",")
                        ),
                    ));
                }
                let want_occupied = pager_occupied.contains(&alias.to_owned());
                if ws.occupied != want_occupied {
                    return Err(fail(
                        at,
                        format!("pager occupied mark is wrong for {alias}"),
                    ));
                }
            } else {
                // Trailing spares never carry occupancy or the current mark.
                if ws.current || ws.occupied {
                    return Err(fail(at, "trailing spare pager marks must stay clear"));
                }
            }
            if !ws.windows.is_empty() {
                return Err(fail(at, "core-only pager paint lists must stay empty"));
            }
        }
    }
    let _ = view;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn submit_v4(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut SimV4,
    command: Option<WorldCommand>,
    command_seq: Option<u64>,
    closed: Vec<String>,
    at: &str,
) -> Result<(), String> {
    sim.req += 1;
    sim.obs += 1;
    let observation_only = command.is_none();
    let report = build_report_v4(case, mode, sim, command, command_seq, closed)?;
    // The test bridge erases Accepted/Duplicate/CommandConsumed into one
    // `Ok` carrying the typed reply; refusals keep their kind/message.
    // Every in-flow submit is fresh, so `Ok` must pair a fresh acceptance:
    // a stale revision below fails instead of masking a consumed command.
    match engine.handle_report_v4_for_test(&report) {
        Ok(reply) => {
            let revision = sim.pairs.len() as u64 + 1;
            check_pairing_v4(case, &report, &reply, at, revision)?;
            note_effects_v4(sim, &reply, at)?;
            // Converge simulated host facts only from settled replies.
            // Command replies with pending transfers carry transient
            // desired frames (possibly overlapping); a real host never
            // reports those as observed frames. Receipts settle first,
            // then the host is converged to the settled allocations.
            if reply.transfers.is_empty() {
                converge_facts_v4(sim, &reply);
            }
            sim.pairs.push((report, reply));
            refresh_away_slot_v4(sim, at)?;
            sim.next_attempts.clear();
            // A spent desire never retries or upgrades: the observation
            // just emitted carried the single correlated verdict.
            if observation_only {
                sim.grants.clear();
            }
            Ok(())
        }
        Err((kind, message)) => Err(format!("{at}: rejected {kind} {message}")),
    }
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn submit_command_v4(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut SimV4,
    action: &RawAction,
    at: &str,
) -> Result<(), String> {
    let binding = binding_of_v4(sim);
    let command = build_command(action, &binding)?.expect("command action");
    let seq = action.seq.expect("validated");
    // The report carries the predicted real foreground unchanged: a
    // subject-carrying command is natively honorable only when the subject
    // already holds the foreground (acquired by an explicit focus-window
    // observation, never silently).
    let mut select_changed = false;
    match action.op.as_str() {
        "select-workspace" => {
            let alias = action.workspace.clone().expect("validated");
            // Changed selections freeze a new binding in the core;
            // redundant ones keep the old authorization. Track the change
            // against the pre-command prediction like the view-change gate.
            select_changed = sim.predicted_alias != alias;
            sim.pending.view = Some((action.output.clone().expect("validated"), alias.clone()));
            sim.predicted_alias = alias;
        }
        "select-numbered" | "select-relative" | "select-previous" => {
            // Core-resolved output lands on the single live screen; the
            // explicit form names it directly.
            let alias = resolve_numbered_relative_v4(action, sim, at)?;
            select_changed = sim.predicted_alias != alias;
            let out = action.output.clone().unwrap_or_else(|| sim.out.clone());
            sim.pending.view = Some((out, alias.clone()));
            sim.predicted_alias = alias;
        }
        "send" => {
            let window = action.window.clone().expect("validated");
            if sim.foreground.as_deref() != Some(window.as_str()) {
                return Err(fail(
                    at,
                    "send subject does not hold the predicted foreground",
                ));
            }
            if sim.outstanding.contains(&window) {
                return Err(fail(at, "transfer-in-flight by construction"));
            }
            if sim.pending.members.contains_key(&window) {
                return Err(fail(at, "second send for a window without a receipt"));
            }
            let alias = action.workspace.clone().expect("validated");
            let target_output = OutputId(action.output.clone().expect("validated"));
            let target_workspace = binding
                .get(&alias)
                .cloned()
                .ok_or_else(|| fail(at, "unbound workspace alias"))?;
            let source_alias = sim
                .homing_alias
                .get(&window)
                .cloned()
                .unwrap_or_else(|| alias.clone());
            let source_workspace = binding
                .get(&source_alias)
                .cloned()
                .ok_or_else(|| fail(at, "unbound workspace alias"))?;
            let operation_id = format!("op-{seq}");
            sim.pending.member_order.push(window.clone());
            sim.pending.members.insert(
                window.clone(),
                PendingMember {
                    operation_id,
                    target_output: target_output.clone(),
                    target_workspace: target_workspace.clone(),
                    pre: Some(WorkspaceRef::Logical {
                        output: target_output.clone(),
                        workspace: WorkspaceId(source_workspace),
                    }),
                    attempt_ms: seq,
                },
            );
            sim.outstanding.insert(window.clone());
            sim.homing_alias.insert(window.clone(), alias.clone());
            if action.follow.expect("validated") {
                sim.pending.view = Some((action.output.clone().expect("validated"), alias.clone()));
                sim.predicted_alias = alias;
            }
        }
        "focus-directional" | "move-directional" => {
            let window = action.window.clone().expect("validated");
            if sim.foreground.as_deref() != Some(window.as_str()) {
                return Err(fail(
                    at,
                    "directional subject does not hold the predicted foreground",
                ));
            }
        }
        other => return Err(fail(at, format!("{other:?} is not a command"))),
    }
    submit_v4(
        case,
        mode,
        engine,
        sim,
        Some(command),
        Some(seq),
        vec![],
        at,
    )?;
    // Foreground transitions on the accepted command: directional effects
    // land on the visible target, follow sends keep the mover foreground,
    // stay sends hide the mover. Selects keep the foreground until the
    // receipt hides it (recomputed there).
    //
    // Changed selections freeze a new visibility binding in the core:
    // record the commanded desire (target, revision) from the actual
    // issuing reply's own visibility rows, pending until the next
    // observation samples the read. This lane is blocked-only: a
    // `Controlled` row fails closed rather than pretending a setter ran.
    // Follow sends prune the binding with no replacement on this lane, so
    // they clear the desire.
    if select_changed {
        sim.grants.clear();
        let reply = &sim.pairs.last().expect("just pushed").1;
        for row in &reply.windows {
            if let Some(target) = &row.visibility {
                if matches!(row.visibility_authority, ScopeAuthority::Controlled) {
                    return Err(fail(
                        at,
                        format!(
                            "controlled visibility for {} is not covered on the blocked lane",
                            row.lifetime.0
                        ),
                    ));
                }
                sim.grants
                    .insert(row.lifetime.0.clone(), (target.shown, target.revision));
            }
        }
    }
    if action.op == "send" && action.follow.expect("validated") {
        sim.grants.clear();
    }
    match action.op.as_str() {
        "focus-directional" | "move-directional" => {
            let target = action.expect_target.clone().expect("validated");
            sim.foreground = Some(target);
        }
        "send" => {
            if action.follow.expect("validated") {
                sim.foreground = Some(action.window.clone().expect("validated"));
            } else {
                sim.foreground = None;
            }
        }
        _ => {}
    }
    let reply = &sim.pairs.last().expect("just pushed").1;
    if action.op == "focus-directional" || action.op == "move-directional" {
        let want_id = action.expect_effect.clone().expect("validated");
        let want_target = action.expect_target.clone().expect("validated");
        let got: Vec<(String, String, String)> = reply
            .effects
            .iter()
            .map(|e| (e.id.clone(), e.kind.as_str().to_owned(), e.target.0.clone()))
            .collect();
        if got != vec![(want_id, "focus".to_owned(), want_target)] {
            return Err(fail(
                at,
                format!("{op} effect {got:?} != expected one-shot", op = action.op),
            ));
        }
    }
    Ok(())
}

/// Predict the view slot of a core-resolved selection: numbered/relative
/// resolve over the two slots from the predicted alias; previous toggles
/// to the other slot. Single output only.
fn resolve_numbered_relative_v4(
    action: &RawAction,
    sim: &SimV4,
    at: &str,
) -> Result<String, String> {
    let slots = ["home", "away"];
    let pos = slots
        .iter()
        .position(|slot| **slot == sim.predicted_alias)
        .ok_or_else(|| fail(at, "predicted alias is not a slot"))?;
    match action.op.as_str() {
        "select-numbered" => {
            let index = action.index.expect("validated") as usize;
            if index < 1 || index > slots.len() {
                return Err(fail(at, "numbered index is outside the slots"));
            }
            Ok(slots[index - 1].to_owned())
        }
        "select-relative" => {
            let delta = action.delta.expect("validated");
            let next = (pos as i32 + delta).rem_euclid(slots.len() as i32) as usize;
            Ok(slots[next].to_owned())
        }
        "select-previous" => {
            // Previous toggles between the two slots; with only home/away
            // the other slot is the target.
            Ok(slots[1 - pos].to_owned())
        }
        other => Err(fail(at, format!("{other:?} has no resolved view"))),
    }
}

/// Emit the first half of a reordered receipt pair. Returns true when a
/// split receipt was emitted and more proofs remain for the follow-up.
/// Single-proof spans emit nothing (nothing to reorder). Split receipts
/// never clear the in-flight window guard; only the settling receipt does.
/// There is no view echo on this lane, so a view-plus-members span emits
/// a membership-proof-free observation receipt first (a fresh observation at
/// the same schedule point, spending desires exactly like the legacy
/// view-echo half), with the membership proofs riding the follow-up.
fn submit_reordered_first_v4(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut SimV4,
    at: &str,
) -> Result<bool, String> {
    let has_view = sim.pending.view.is_some();
    let members = sim.pending.members.len();
    if has_view && members > 0 {
        // Observation half first, membership proofs on the follow-up.
        // The predicted alias already steers the next command; the receipt
        // itself carries no view ingress on this lane.
        sim.pending.view = None;
        submit_v4(case, mode, engine, sim, None, None, vec![], at)?;
        recompute_foreground_v4(sim);
        return Ok(true);
    }
    if members >= 2 {
        let n = sim.pending.member_order.len();
        let start = n / 2;
        // Apply later-half membership proofs now; the rest ride the
        // follow-up receipt. Later commands prove first. Each proof
        // carries the command-time target/stamp from the actual simulated
        // write, never a re-resolved desired state.
        let later: Vec<String> = sim.pending.member_order[start..].to_vec();
        let mut proofs = Vec::new();
        for window in &later {
            if let Some(member) = sim.pending.members.remove(window) {
                proofs.push(member_proof(window, &member));
            }
        }
        sim.pending.member_order.drain(start..);
        sim.next_attempts.extend(proofs);
        submit_v4(case, mode, engine, sim, None, None, vec![], at)?;
        recompute_foreground_v4(sim);
        return Ok(true);
    }
    Ok(false)
}

/// Emit one observation report carrying all pending proofs. No view echo:
/// receipts carry only truthful membership proofs plus the correlated
/// visibility verdicts sampled from the raw facts.
fn submit_receipt_v4(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut SimV4,
    at: &str,
) -> Result<(), String> {
    {
        // Truthful raw proofs for every simulated membership actuation:
        // same window/target/stamp as the command-time write.
        let mut proofs = Vec::new();
        for (window, member) in sim.pending.members.iter() {
            proofs.push(member_proof(window, member));
        }
        sim.next_attempts.extend(proofs);
    }
    sim.pending = Pending::empty();
    submit_v4(case, mode, engine, sim, None, None, vec![], at)?;
    sim.outstanding.clear();
    recompute_foreground_v4(sim);
    Ok(())
}

fn run_stress_v4(
    case: &StressCase,
    schedule: StressSchedule,
) -> Result<(Vec<String>, StressStats), String> {
    let mode = case_mode(&case.mode)?;
    let binding = bootstrap_binding_v4(case, mode)?;
    let mut engine = WorldEngine::new(&case.generation).ok_or("bad generation")?;
    bind_engine(&mut engine, case, mode)?;
    let mut sim = SimV4 {
        req: 0,
        obs: 0,
        facts: case.initial_facts.clone(),
        live: case.initial_facts.keys().cloned().collect(),
        pending: Pending::empty(),
        next_attempts: Vec::new(),
        outstanding: BTreeSet::new(),
        home: binding["home"].clone(),
        away: binding["away"].clone(),
        seen: BTreeSet::new(),
        out: case.screens[0].out.clone(),
        predicted_alias: "home".to_owned(),
        foreground: None,
        homing_alias: case
            .initial_facts
            .keys()
            .map(|w| (w.clone(), "home".to_owned()))
            .collect(),
        focus: String::new(),
        grants: BTreeMap::new(),
        pairs: Vec::new(),
    };
    let mut lines = Vec::new();
    // Bootstrap observation: roster homed home, tiles equal the authored
    // converged facts, no transfers, pager current home. No managed window
    // holds the foreground yet.
    submit_v4(
        case,
        mode,
        &mut engine,
        &mut sim,
        None,
        None,
        vec![],
        "bootstrap",
    )?;
    {
        let reply = &sim.pairs.last().expect("bootstrap").1.clone();
        let members: Vec<String> = sim.live.iter().cloned().collect();
        let homing: BTreeMap<String, String> = members
            .iter()
            .map(|w| (w.clone(), "home".to_owned()))
            .collect();
        check_oracle_v4(
            case,
            &sim,
            reply,
            "bootstrap",
            "home",
            &members,
            &homing,
            Some(&case.initial_facts),
            "",
            &None,
            "home",
            &["home".to_owned()],
            false,
        )?;
        lines.push(format!(
            "ok {} bootstrap req=req-1 rev=1 view=home members={}",
            schedule.as_str(),
            members.len()
        ));
    }
    let mut checkpoints = 0;
    for (i, action) in case.actions.iter().enumerate() {
        let at = format!("action[{i}] {}", action.op);
        match action.op.as_str() {
            "checkpoint" => {
                if schedule != StressSchedule::Ordered {
                    if schedule == StressSchedule::Reordered
                        && !sim.pending.is_empty()
                        && submit_reordered_first_v4(case, mode, &mut engine, &mut sim, &at)?
                    {
                        let reply = &sim.pairs.last().expect("receipt").1;
                        if reply.transfers.is_empty() {
                            return Err(fail(&at, "reordered split receipt settled early"));
                        }
                    }
                    submit_receipt_v4(case, mode, &mut engine, &mut sim, &at)?;
                }
                let reply = &sim.pairs.last().expect("checkpoint").1.clone();
                let members = action.members.clone().expect("validated");
                let homing = action.homing.clone().expect("validated");
                let foreground = action.foreground.clone().flatten();
                check_oracle_v4(
                    case,
                    &sim,
                    reply,
                    &at,
                    action.view.as_ref().expect("validated"),
                    &members,
                    &homing,
                    action.tiles.as_ref(),
                    action.focus.as_ref().expect("validated"),
                    &foreground,
                    action.pager_current.as_ref().expect("validated"),
                    action.pager_occupied.as_ref().expect("validated"),
                    true,
                )?;
                checkpoints += 1;
                lines.push(format!("ok {} {at}", schedule.as_str()));
            }
            "admit" => {
                let window = action.window.clone().expect("validated");
                if schedule != StressSchedule::Ordered {
                    // Batched/reordered admissions piggyback pending proofs.
                    let mut proofs = Vec::new();
                    for (w, member) in sim.pending.members.iter() {
                        proofs.push(member_proof(w, member));
                    }
                    sim.next_attempts.extend(proofs);
                    sim.pending = Pending::empty();
                }
                sim.facts
                    .insert(window.clone(), action.rect.expect("validated"));
                sim.live.insert(window.clone());
                // Admission homes to the shown view and never arrives with
                // the foreground: the next focus-window observation (or the
                // next receipt, when none is possible) owns the truth.
                sim.homing_alias
                    .insert(window.clone(), sim.predicted_alias.clone());
                sim.foreground = None;
                submit_v4(case, mode, &mut engine, &mut sim, None, None, vec![], &at)?;
                sim.outstanding.clear();
                if schedule == StressSchedule::Ordered {
                    let reply = &sim.pairs.last().expect("admit").1;
                    if !reply.transfers.is_empty() {
                        return Err(fail(&at, "ordered admit left a transfer"));
                    }
                }
            }
            "close" => {
                let window = action.window.clone().expect("validated");
                if schedule != StressSchedule::Ordered {
                    let mut proofs = Vec::new();
                    for (w, member) in sim.pending.members.iter() {
                        if w != &window {
                            proofs.push(member_proof(w, member));
                        }
                    }
                    sim.next_attempts.extend(proofs);
                    sim.pending = Pending::empty();
                }
                sim.facts.remove(&window);
                sim.live.remove(&window);
                sim.homing_alias.remove(&window);
                if sim.foreground.as_deref() == Some(window.as_str()) {
                    sim.foreground = None;
                }
                submit_v4(
                    case,
                    mode,
                    &mut engine,
                    &mut sim,
                    None,
                    None,
                    vec![window],
                    &at,
                )?;
                sim.outstanding.clear();
                if schedule == StressSchedule::Ordered {
                    let reply = &sim.pairs.last().expect("close").1;
                    if !reply.transfers.is_empty() {
                        return Err(fail(&at, "ordered close left a transfer"));
                    }
                }
            }
            "focus-window" => {
                let window = action.window.clone().expect("validated");
                if !sim.live.contains(&window) {
                    return Err(fail(&at, "focus-window of a non-live window"));
                }
                if sim.homing_alias.get(&window).cloned().unwrap_or_default() != sim.predicted_alias
                {
                    return Err(fail(&at, "focus-window outside the shown view"));
                }
                if schedule != StressSchedule::Ordered {
                    // Explicit focus observations flush pending proofs like
                    // admit/close: the acquisition reads settled visibility.
                    let mut proofs = Vec::new();
                    for (w, member) in sim.pending.members.iter() {
                        proofs.push(member_proof(w, member));
                    }
                    sim.next_attempts.extend(proofs);
                    sim.pending = Pending::empty();
                }
                sim.foreground = Some(window);
                submit_v4(case, mode, &mut engine, &mut sim, None, None, vec![], &at)?;
                sim.outstanding.clear();
                if schedule == StressSchedule::Ordered {
                    let reply = &sim.pairs.last().expect("focus-window").1;
                    if !reply.transfers.is_empty() {
                        return Err(fail(&at, "ordered focus-window left a transfer"));
                    }
                }
            }
            _ => {
                submit_command_v4(case, mode, &mut engine, &mut sim, action, &at)?;
                if schedule == StressSchedule::Ordered {
                    // Settle every action immediately in the ordered mode.
                    let needs_receipt = !sim.pending.is_empty();
                    if needs_receipt {
                        submit_receipt_v4(case, mode, &mut engine, &mut sim, &at)?;
                        let reply = &sim.pairs.last().expect("receipt").1;
                        if !reply.transfers.is_empty() {
                            return Err(fail(&at, "ordered receipt left a transfer"));
                        }
                    } else {
                        let reply = &sim.pairs.last().expect("command").1;
                        if !reply.transfers.is_empty() {
                            return Err(fail(&at, "one-shot command left a transfer"));
                        }
                    }
                }
            }
        }
    }
    // Final receipt plus the hand-authored final oracle.
    if schedule == StressSchedule::Reordered && !sim.pending.is_empty() {
        let _ = submit_reordered_first_v4(case, mode, &mut engine, &mut sim, "final-split")?;
    }
    if !sim.pending.is_empty() {
        submit_receipt_v4(case, mode, &mut engine, &mut sim, "final")?;
    }
    // In ordered mode the last receipt already settled; still emit a final
    // observation so the oracle reads one fresh settled reply.
    if schedule == StressSchedule::Ordered {
        submit_v4(
            case,
            mode,
            &mut engine,
            &mut sim,
            None,
            None,
            vec![],
            "final",
        )?;
    }
    {
        let reply = &sim.pairs.last().expect("final").1.clone();
        let end = &case.final_expect;
        check_oracle_v4(
            case,
            &sim,
            reply,
            "final",
            &end.view,
            &end.members,
            &end.homing,
            Some(&end.tiles),
            &end.focus,
            &end.foreground,
            &end.pager_current,
            &end.pager_occupied,
            true,
        )?;
    }
    // Replay guards hold in every schedule. The bridge erases intent
    // outcomes into one `Ok`, so the probes assert the erased shape:
    // exact redelivery returns the cached reply with no new revision,
    // an already-consumed command reconciles into a fresh reply exactly
    // one revision later with the genuine `command_consumed` bit set,
    // and stale or reused requests keep their exact refusal kinds.
    probe_completions_v4(case, &mut engine, &mut sim)?;
    lines.push(format!(
        "ok {} final members={} reports={}",
        schedule.as_str(),
        case.final_expect.members.len(),
        sim.pairs.len()
    ));
    Ok((
        lines,
        StressStats {
            reports: sim.pairs.len(),
            checkpoints,
        },
    ))
}

/// Duplicate redelivery returns the cached reply, stale-observation and
/// reused-request completions refuse, and an already-consumed command
/// reconciles without re-execution.
fn probe_completions_v4(
    case: &StressCase,
    engine: &mut WorldEngine,
    sim: &mut SimV4,
) -> Result<(), String> {
    let (last_report, last_reply) = sim.pairs.last().expect("final").clone();
    // Exact redelivery is a duplicate: the cached reply returns with no new
    // revision, never a fresh acceptance.
    match engine.handle_report_v4_for_test(&last_report) {
        Ok(cached) => {
            if cached != last_reply {
                return Err("probe duplicate: cached reply moved".to_owned());
            }
            if cached.state_revision != last_reply.state_revision {
                return Err("probe duplicate: cached revision moved".to_owned());
            }
        }
        Err((kind, message)) => {
            return Err(format!(
                "probe duplicate: expected cached duplicate, got {kind} {message}"
            ));
        }
    }
    // Fresh request sequence with a stale observation refuses.
    {
        let mut stale = last_report.clone();
        stale.request_seq = sim.req + 1;
        stale.request_id = format!("req-{}", sim.req + 1);
        stale.observation_seq = sim.obs.saturating_sub(1).max(1);
        match engine.handle_report_v4_for_test(&stale) {
            Err(("observation-stale", _)) => {}
            Err((kind, message)) => {
                return Err(format!(
                    "probe stale-obs: expected observation-stale, got {kind} {message}"
                ));
            }
            Ok(_) => {
                return Err("probe stale-obs: expected observation-stale, accepted".to_owned());
            }
        }
    }
    // Reused request sequence with different input refuses.
    {
        let mut reused = last_report.clone();
        reused.focused_global = "nobody".to_owned();
        match engine.handle_report_v4_for_test(&reused) {
            Err(("request-reused", _)) => {}
            Err((kind, message)) => {
                return Err(format!(
                    "probe reused-req: expected request-reused, got {kind} {message}"
                ));
            }
            Ok(_) => {
                return Err("probe reused-req: expected request-reused, accepted".to_owned());
            }
        }
    }
    // An already-consumed command sequence never re-executes: the
    // observation reconciles into a fresh reply exactly one revision
    // later, with the genuine `command_consumed` bit set and the command
    // never dispatched. The bridge erases the intent outcome into `Ok`,
    // so the probe asserts the erased shape on genuine reply rows.
    {
        let first_seq = case
            .actions
            .iter()
            .filter_map(|a| a.seq)
            .min()
            .ok_or("probe consumed: case has no command seq")?;
        let binding = binding_of_v4(sim);
        let first = case
            .actions
            .iter()
            .find(|a| a.seq == Some(first_seq))
            .expect("seq");
        let command = build_command(first, &binding)?.expect("command");
        let mode = case_mode(&case.mode)?;
        let probe_sim = SimV4 {
            req: sim.req + 1,
            obs: sim.obs + 1,
            facts: sim.facts.clone(),
            live: sim.live.clone(),
            pending: Pending::empty(),
            next_attempts: Vec::new(),
            outstanding: BTreeSet::new(),
            home: sim.home.clone(),
            away: sim.away.clone(),
            seen: sim.seen.clone(),
            out: sim.out.clone(),
            predicted_alias: sim.predicted_alias.clone(),
            foreground: sim.foreground.clone(),
            homing_alias: sim.homing_alias.clone(),
            focus: sim.focus.clone(),
            grants: BTreeMap::new(),
            pairs: Vec::new(),
        };
        let report = build_report_v4(
            case,
            mode,
            &probe_sim,
            Some(command),
            Some(first_seq),
            vec![],
        )?;
        match engine.handle_report_v4_for_test(&report) {
            Ok(reply) => {
                if !reply.command_consumed {
                    return Err(
                        "probe consumed-cmd: expected command_consumed set, re-executed".to_owned(),
                    );
                }
                if reply.state_revision != last_reply.state_revision + 1 {
                    return Err(format!(
                        "probe consumed-cmd: expected exactly one reconcile revision, got {} != {}",
                        reply.state_revision,
                        last_reply.state_revision + 1
                    ));
                }
            }
            Err((kind, message)) => {
                return Err(format!(
                    "probe consumed-cmd: expected consumed without re-execution, got {kind} {message}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duo_path() -> String {
        format!(
            "{}/../../test-fixtures/world-stress-home-away.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn trio_path() -> String {
        format!(
            "{}/../../test-fixtures/world-stress-trio-churn.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    /// Minimal real-path drive to duo seq16 (first changed select, home to
    /// away): bootstrap plus the one command, so verdict derivation reads
    /// the actual issuing reply instead of a fabricated shape.
    fn drive_to_seq16() -> (StressCase, WorkspaceMode, WorldEngine, SimV4) {
        let case = load_stress_case(&duo_path()).expect("load duo");
        let mode = case_mode(&case.mode).expect("mode");
        let binding = bootstrap_binding_v4(&case, mode).expect("binding");
        let mut engine = WorldEngine::new(&case.generation).expect("generation");
        bind_engine(&mut engine, &case, mode).expect("bind");
        let mut sim = SimV4 {
            req: 0,
            obs: 0,
            facts: case.initial_facts.clone(),
            live: case.initial_facts.keys().cloned().collect(),
            pending: Pending::empty(),
            next_attempts: Vec::new(),
            outstanding: BTreeSet::new(),
            home: binding["home"].clone(),
            away: binding["away"].clone(),
            seen: BTreeSet::new(),
            out: case.screens[0].out.clone(),
            predicted_alias: "home".to_owned(),
            foreground: None,
            homing_alias: case
                .initial_facts
                .keys()
                .map(|w| (w.clone(), "home".to_owned()))
                .collect(),
            focus: String::new(),
            grants: BTreeMap::new(),
            pairs: Vec::new(),
        };
        submit_v4(
            &case,
            mode,
            &mut engine,
            &mut sim,
            None,
            None,
            vec![],
            "bootstrap",
        )
        .expect("bootstrap");
        let action = case
            .actions
            .iter()
            .find(|a| a.seq == Some(16))
            .expect("seq16")
            .clone();
        assert_eq!(action.op, "select-workspace");
        submit_command_v4(&case, mode, &mut engine, &mut sim, &action, "seq16").expect("select");
        (case, mode, engine, sim)
    }

    #[test]
    fn select_verdicts_are_blocked_nocalls_sampled_at_the_receipt_v4() {
        use tiler_core::world_report::{SwitchFocus, SwitchOutcome};
        let (case, mode, mut engine, mut sim) = drive_to_seq16();
        // Desire recorded from the actual issuing reply rows, never a live
        // guess: target and revision per row.
        let reply = &sim.pairs.last().expect("select reply").1;
        assert!(!sim.grants.is_empty(), "changed select records desire");
        for row in &reply.windows {
            let target = row
                .visibility
                .as_ref()
                .expect("row carries a commanded visibility target");
            let (shown, rev) = sim
                .grants
                .get(&row.lifetime.0)
                .expect("grant per reply row");
            assert_eq!((*shown, *rev), (target.shown, target.revision));
            assert!(
                !matches!(row.visibility_authority, ScopeAuthority::Controlled),
                "blocked lane never grants write authority"
            );
        }
        // The command report itself proves nothing.
        let cmd_report = &sim.pairs.last().expect("select report").0;
        assert!(
            cmd_report.windows.iter().all(|w| w.last_write.is_none()),
            "no premature verdict on the command report"
        );
        // Probe: a redundant select (no supersession) carries no verdict
        // and leaves the recorded desire unconsumed for the real receipt.
        let redundant = RawAction {
            op: "select-workspace".to_owned(),
            seq: Some(999),
            output: Some(case.screens[0].out.clone()),
            workspace: Some("away".to_owned()),
            index: None,
            delta: None,
            window: None,
            direction: None,
            same_axis: None,
            follow: None,
            rect: None,
            name: None,
            view: None,
            members: None,
            homing: None,
            tiles: None,
            focus: None,
            foreground: None,
            expect_effect: None,
            expect_target: None,
            pager_current: None,
            pager_occupied: None,
        };
        let grants_before = sim.grants.clone();
        submit_command_v4(&case, mode, &mut engine, &mut sim, &redundant, "redundant")
            .expect("redundant select");
        let cmd_report = &sim.pairs.last().expect("redundant report").0;
        assert!(
            cmd_report.windows.iter().all(|w| w.last_write.is_none()),
            "commands never carry verdicts"
        );
        assert_eq!(
            sim.grants, grants_before,
            "commands never consume the desire"
        );
        // The original receipt boundary emits the verdict once, sampled at
        // the receipt observation from the raw facts.
        let obs_before = sim.obs;
        submit_receipt_v4(&case, mode, &mut engine, &mut sim, "receipt").expect("receipt");
        let (report, reply) = sim.pairs.last().expect("receipt pair");
        assert_eq!(report.observation_seq, obs_before + 1);
        for window in &report.windows {
            let (shown, rev) = grants_before.get(&window.window.0).expect("spent grant");
            let last = window
                .last_write
                .as_ref()
                .expect("verdict rides the receipt");
            assert_eq!(
                last.property,
                tiler_core::world_report::LastWriteProperty::Visibility
            );
            assert_eq!(last.target, *shown);
            assert_eq!(last.revision, *rev);
            assert_eq!(
                last.setter,
                tiler_core::world_report::VisibilitySetter::NotCalled
            );
            assert_eq!(
                last.reason,
                tiler_core::world_report::VisibilityReason::ScopeHeld
            );
            let post = last.post.as_ref().expect("no-call still samples the read");
            assert_eq!(
                post.obs, report.observation_seq,
                "read watermarked at this receipt, never an older observation"
            );
            assert_eq!(
                post.visibility, window.visibility,
                "sample agrees the raw facts"
            );
            assert_eq!(post.lifetime_matched, Some(true));
            assert_eq!(post.minimized, Some(false));
            assert_eq!(post.host_hidden, Some(false));
        }
        assert!(sim.grants.is_empty(), "first receipt spends the desire");
        // The common terminal still fires: this isolated drive never ran
        // the prefix sends, so both windows are homed home and the
        // away-select hides both. Neither hide can read back hidden while
        // the raw facts stay visible, so both fail; focus stays unproven
        // on this focusless lane.
        assert_eq!(reply.switch_settled.len(), 1);
        let event = &reply.switch_settled[0];
        assert_eq!(event.operation_id, "op-16");
        assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
        assert_eq!(
            event.failed,
            vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())]
        );
        assert_eq!(event.focus, SwitchFocus::Unproven);
        // A second ordinary observation retries nothing: the spent desire
        // is never re-emitted, the latched terminal never redelivers, and
        // no second terminal follows from a retry.
        submit_v4(
            &case,
            mode,
            &mut engine,
            &mut sim,
            None,
            None,
            vec![],
            "again",
        )
        .expect("again");
        let (report, reply) = sim.pairs.last().expect("second observation");
        assert!(
            report.windows.iter().all(|w| w.last_write.is_none()),
            "spent verdicts are not re-emitted"
        );
        assert!(
            reply.switch_settled.is_empty(),
            "no second terminal from a retry"
        );
        assert!(sim.grants.is_empty(), "desire stays spent");
    }

    #[test]
    fn accepts_duo_ordered_v4() {
        let case = load_stress_case(&duo_path()).expect("load duo");
        let command_ops = case.actions.iter().filter(|a| a.seq.is_some()).count();
        assert!(command_ops >= 100, "stress volume {command_ops} < 100");
        let (lines, stats) = run_stress_v4(&case, StressSchedule::Ordered).expect("ordered");
        assert_eq!(stats.reports, 186);
        assert_eq!(stats.checkpoints, 16);
        assert!(lines.iter().any(|l| l.contains("final members=2")));
    }

    #[test]
    fn accepts_duo_batched_v4() {
        let case = load_stress_case(&duo_path()).expect("load duo");
        let (_, stats) = run_stress_v4(&case, StressSchedule::Batched).expect("batched");
        assert_eq!(stats.reports, 149);
        assert_eq!(stats.checkpoints, 16);
    }

    #[test]
    fn accepts_duo_reordered_v4() {
        let case = load_stress_case(&duo_path()).expect("load duo");
        let (_, stats) = run_stress_v4(&case, StressSchedule::Reordered).expect("reordered");
        assert_eq!(stats.reports, 151);
        assert_eq!(stats.checkpoints, 16);
    }

    #[test]
    fn accepts_trio_ordered_v4() {
        let case = load_stress_case(&trio_path()).expect("load trio");
        let command_ops = case.actions.iter().filter(|a| a.seq.is_some()).count();
        assert!(command_ops >= 100, "stress volume {command_ops} < 100");
        let (lines, stats) = run_stress_v4(&case, StressSchedule::Ordered).expect("ordered");
        assert_eq!(stats.reports, 190);
        assert_eq!(stats.checkpoints, 19);
        assert!(lines.iter().any(|l| l.contains("final members=3")));
    }

    #[test]
    fn accepts_trio_batched_v4() {
        let case = load_stress_case(&trio_path()).expect("load trio");
        let (_, stats) = run_stress_v4(&case, StressSchedule::Batched).expect("batched");
        assert_eq!(stats.reports, 156);
        assert_eq!(stats.checkpoints, 19);
    }

    #[test]
    fn accepts_trio_reordered_v4() {
        let case = load_stress_case(&trio_path()).expect("load trio");
        let (_, stats) = run_stress_v4(&case, StressSchedule::Reordered).expect("reordered");
        assert_eq!(stats.reports, 158);
        assert_eq!(stats.checkpoints, 19);
    }
}
