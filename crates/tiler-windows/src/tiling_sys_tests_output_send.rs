// Windows explicit output send (REQ-OUT-04, decisions 5.3/5.4) through a
// real `TileLoop` against the real retained Engine (no HWNDs, hooks, or
// live effects): four-direction FULL-rectangle selection with stacked/
// negative/panel-gap topologies, empty/occupied current views, ordinary
// admission, follow/stay/source-null focus, floating boundaries with
// tiled-side-only reflow, sticky/float exclusion, stale topology/view/
// member/lifetime refusals, and late native failures through the existing
// injected-area/apply/observe/focus seams (never faked OS successes).
// Native setters stay user-owned live acceptance; these pin the DISTINCT
// `SendToOutput` construction, `SendOutput` extraction, commit-last
// transfer verification, and project membership/focus settlement.
use super::rmax03_adapter_tests::test_state;
use super::{
    HintCx, MonitorArea, ObservedWindow, clear_pending_seams, inject_monitor_areas,
    inject_pending_apply, inject_pending_focus, inject_pending_observe, inject_pending_tag,
    output_for_rect, pending_apply_writables, pending_counters, resolve_output_send_target,
    run_output_send, workspace_domain_for,
};
use crate::snapkey::{QueuedWorkspaceOutputIntent, SnapEdge, SnapOrigin};
use crate::workspace::{MemberLoc, WindowKey};
use std::collections::BTreeSet;
use tiler_core::directional::{Direction, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

fn area(device: &str, work: Rect, full: Rect) -> MonitorArea {
    MonitorArea {
        device: device.to_owned(),
        work,
        full,
    }
}

fn full_rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn triple_chain() -> Vec<MonitorArea> {
    vec![
        area(
            "primary",
            full_rect(0, 40, 1920, 1040),
            full_rect(0, 0, 1920, 1080),
        ),
        area(
            "right",
            full_rect(1920, 0, 1920, 1080),
            full_rect(1920, 0, 1920, 1080),
        ),
        area(
            "far",
            full_rect(3840, 0, 1920, 1080),
            full_rect(3840, 0, 1920, 1080),
        ),
    ]
}

fn key(hwnd: u64, pid: u32) -> WindowKey {
    WindowKey {
        hwnd,
        pid,
        creation: format!("c{hwnd:016x}"),
    }
}

fn me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 5001,
        process_creation: "creation-me".to_owned(),
        user_sid: "S-1-output".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    }
}

fn observed_window(
    key: &WindowKey,
    token: &str,
    frame: Rect,
    me: &crate::model::ProcessIdentity,
) -> ObservedWindow {
    ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

fn seed(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    bounds: Rect,
    members: &[&str],
    focused: &str,
) {
    let domain = crate::workspace_owner::workspace_domain(output, workspace, bounds, 8);
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = members
        .iter()
        .map(|token| {
            (
                WindowId((*token).to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            )
        })
        .collect();
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("output-send-seed").expect("correlation");
    let fp = crate::tiling::fingerprint(
        &members
            .iter()
            .map(|t| ((*t).to_owned(), bounds))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed must converge, got {reply:?}"
    );
}

struct OutputSendRig {
    state: super::TileLoop,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    me: crate::model::ProcessIdentity,
    mover: WindowKey,
    home: MemberLoc,
    target_output: String,
    target_ws: String,
    observed: Vec<ObservedWindow>,
    source_rows: Vec<crate::workspace_owner::OwnerRow>,
    intent: QueuedWorkspaceOutputIntent,
    origin: SnapOrigin,
}

/// Full-path rig: project members plus Engine sessions on `primary` and
/// `right`, with an inert `far` output that no send may touch. Source and
/// target members ride distinct fake HWNDs with fresh frames so row
/// assembly stays complete headless.
#[allow(clippy::too_many_arguments)]
fn rig(
    follow: bool,
    direction: Direction,
    source_members: &[&str],
    target_members: &[&str],
    target_tiled: bool,
) -> OutputSendRig {
    use super::{inject_pending_elevated, inject_pending_suspend, inject_pending_tag};
    clear_pending_seams();
    let mut state = test_state();
    let areas = triple_chain();
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("primary");
    state.workspaces.ensure_output("right");
    state.workspaces.ensure_output("far");
    let ws_a = state.workspaces.active_id("primary").expect("active");
    let ws_b = state.workspaces.active_id("right").expect("active");
    let ws_far = state.workspaces.active_id("far").expect("active");
    state.workspaces.set_tiled("primary", &ws_a, true);
    state.workspaces.set_tiled("right", &ws_b, target_tiled);
    let src_bounds = workspace_domain_for("primary", &ws_a, &areas, 8, 8)
        .expect("src domain")
        .0
        .bounds;
    let dst_bounds = workspace_domain_for("right", &ws_b, &areas, 8, 8)
        .expect("dst domain")
        .0
        .bounds;
    let me = me();
    let mut members: Vec<(WindowKey, String, Rect, String, String)> = Vec::new();
    for (i, (token, output, ws)) in source_members
        .iter()
        .map(|t| (*t, "primary", ws_a.clone()))
        .chain(target_members.iter().map(|t| (*t, "right", ws_b.clone())))
        .enumerate()
    {
        let k = key(0x5100u64 + i as u64, 8100u32 + i as u32);
        let frame = if output == "primary" {
            Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            }
        } else {
            Rect {
                x: 2020,
                y: 100,
                w: 400,
                h: 300,
            }
        };
        members.push((k, token.to_owned(), frame, output.to_owned(), ws));
    }
    for (k, token, _frame, output, ws) in &members {
        state.member_tokens.insert(k.clone(), token.clone());
        state.member_identity.insert(
            k.clone(),
            crate::model::ProcessIdentity {
                pid: k.pid,
                process_creation: k.creation.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state
            .member_tags
            .insert(k.clone(), format!("tag-{:x}", k.hwnd));
        inject_pending_tag(k.hwnd, Some(format!("tag-{:x}", k.hwnd)));
        assert!(state.workspaces.assign(k.clone(), output, ws, false));
    }
    let mover = members.first().expect("source mover").0.clone();
    let home = MemberLoc {
        output: "primary".to_owned(),
        workspace: ws_a.clone(),
        hidden: false,
    };
    let source_tokens: Vec<&str> = source_members.to_vec();
    seed(
        &mut state,
        "primary",
        &ws_a,
        src_bounds,
        &source_tokens,
        source_tokens[0],
    );
    if !target_members.is_empty() {
        seed(
            &mut state,
            "right",
            &ws_b,
            dst_bounds,
            target_members,
            target_members[0],
        );
    }
    let observed: Vec<ObservedWindow> = members
        .iter()
        .map(|(k, token, frame, _, _)| observed_window(k, token, *frame, &me))
        .collect();
    let source_rows: Vec<crate::workspace_owner::OwnerRow> = source_members
        .iter()
        .map(|token| crate::workspace_owner::OwnerRow {
            token: (*token).to_owned(),
            rect: Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        })
        .collect();
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    let _ = (ws_far, dst_bounds);
    OutputSendRig {
        state,
        areas,
        fulls,
        me,
        mover: mover.clone(),
        home,
        target_output: "right".to_owned(),
        target_ws: ws_b,
        observed,
        source_rows,
        intent: QueuedWorkspaceOutputIntent {
            direction,
            follow,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        },
        origin: SnapOrigin {
            hwnd: mover.hwnd,
            token: source_members[0].to_owned(),
            pid: mover.pid,
            creation: mover.creation.clone(),
        },
    }
}

fn run_send(rig: &mut OutputSendRig, correlation: &CorrelationId, tick: u64) -> &'static str {
    let mut hint_cx = HintCx::new();
    run_output_send(
        &mut rig.state,
        &rig.me,
        &rig.fulls,
        &rig.areas,
        &rig.observed,
        &[],
        &mut hint_cx,
        &rig.home,
        &rig.mover,
        "mover",
        &rig.source_rows,
        &rig.target_output.clone(),
        &rig.target_ws.clone(),
        &rig.intent,
        &rig.origin,
        tick,
        correlation,
        correlation.as_str(),
    )
}

/// Predict the mover's planned arrival rect by replaying the production
/// `SendToOutput` construction on a cloned Engine (no mutation): success
/// tests stage it for the arrival proof.
fn predict_send_output_mover_rect(rig: &OutputSendRig, correlation: &CorrelationId) -> Rect {
    use super::{revision_for, stamp_engine_event};
    let (source_base, source_key) =
        workspace_domain_for(&rig.home.output, &rig.home.workspace, &rig.areas, 8, 8)
            .expect("source domain");
    let (target_base, target_key) =
        workspace_domain_for(&rig.target_output, &rig.target_ws, &rig.areas, 8, 8)
            .expect("target domain");
    let target_rows: Vec<crate::workspace_owner::OwnerRow> = rig
        .state
        .workspaces
        .workspace_members(&rig.target_output, &rig.target_ws)
        .iter()
        .filter_map(|k| rig.state.member_tokens.get(k).map(|t| (k, t)))
        .map(|(_, token)| crate::workspace_owner::OwnerRow {
            token: token.clone(),
            rect: Rect {
                x: 2020,
                y: 100,
                w: 400,
                h: 300,
            },
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        })
        .collect();
    let mut pairs: Vec<(String, Rect)> = rig
        .source_rows
        .iter()
        .chain(target_rows.iter())
        .map(|r| (r.token.clone(), r.rect))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let fp = crate::tiling::fingerprint(&pairs);
    let mut event = crate::workspace_owner::build_send_output_event(
        &rig.state.owner,
        &rig.state.generation,
        correlation,
        revision_for(&rig.state, &rig.home.output, &rig.home.workspace),
        fp,
        (source_base, source_key.clone()),
        (target_base, target_key.clone()),
        &rig.source_rows,
        &target_rows,
        "mover",
        rig.state.outer_gap,
        rig.intent.follow,
    )
    .expect("output event");
    crate::workspace_owner::stamp_send_output_target(&mut event, &target_key);
    stamp_engine_event(&rig.state, &mut event);
    let mut candidate = rig.state.engine.clone();
    let reply = candidate.handle(&event);
    let tiler_core::boundary::CoreReply::SendOutput(_) = &reply else {
        panic!("output send must plan, got {reply:?}");
    };
    crate::workspace_owner::planned_writes(&reply)
        .expect("SendOutput extracts")
        .iter()
        .find(|w| w.window.0 == "mover")
        .expect("mover write")
        .rect
}

fn stage_success(rig: &mut OutputSendRig, correlation: &CorrelationId) {
    let planned = predict_send_output_mover_rect(rig, correlation);
    assert_eq!(
        output_for_rect(&rig.areas, &planned),
        "right",
        "planned arrival must land on the destination monitor"
    );
    rig.state.stable.insert(rig.mover.hwnd, planned);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
}

#[test]
fn follow_empty_target_commits_once_leaving_third_output_inert() {
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let correlation = CorrelationId::parse("corr-send-follow").expect("correlation");
    stage_success(&mut rig, &correlation);
    let active_before: Vec<(String, Option<String>)> = ["primary", "right", "far"]
        .iter()
        .map(|o| ((*o).to_owned(), rig.state.workspaces.active_id(o)))
        .collect();
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 31);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right");
    assert_eq!(loc.workspace, rig.target_ws);
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "verified transfer publishes the Engine once"
    );
    assert_eq!(
        pending_counters().focus,
        1,
        "verified follow focuses the mover exactly once"
    );
    for (output, view) in &active_before {
        assert_eq!(
            rig.state.workspaces.active_id(output).as_ref(),
            view.as_ref(),
            "no current view switches on {output}"
        );
    }
    assert!(
        rig.state
            .workspaces
            .workspace_members(
                "far",
                &rig.state.workspaces.active_id("far").expect("far view")
            )
            .is_empty(),
        "third output stays inert"
    );
    clear_pending_seams();
}

#[test]
fn occupied_target_follow_keeps_current_views() {
    let mut rig = rig(true, Direction::Right, &["mover"], &["t1", "t2"], true);
    let correlation = CorrelationId::parse("corr-send-occupied").expect("correlation");
    stage_success(&mut rig, &correlation);
    let ws_a = rig.home.workspace.clone();
    let ws_b = rig.target_ws.clone();
    let outcome = run_send(&mut rig, &correlation, 32);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(
        (loc.output.as_str(), loc.workspace.as_str()),
        ("right", ws_b.as_str())
    );
    assert_eq!(
        rig.state.workspaces.active_id("primary").as_deref(),
        Some(ws_a.as_str()),
        "source view unchanged"
    );
    assert_eq!(
        rig.state.workspaces.active_id("right").as_deref(),
        Some(ws_b.as_str()),
        "destination current view unchanged"
    );
    clear_pending_seams();
}

#[test]
fn stay_focuses_source_mru() {
    let mut rig = rig(false, Direction::Right, &["mover", "s1"], &[], true);
    let correlation = CorrelationId::parse("corr-send-stay-mru").expect("correlation");
    stage_success(&mut rig, &correlation);
    let outcome = run_send(&mut rig, &correlation, 33);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right", "mover transferred on stay");
    let survivor = rig
        .state
        .member_tokens
        .iter()
        .find(|(_, t)| t.as_str() == "s1")
        .map(|(k, _)| k.clone())
        .expect("survivor key");
    assert_eq!(
        rig.state.workspaces.focus_target(
            "primary",
            &rig.home.workspace,
            &[survivor.clone()].into_iter().collect::<BTreeSet<_>>()
        ),
        Some(survivor),
        "source MRU holds stay focus"
    );
    assert_eq!(
        pending_counters().focus,
        1,
        "stay actuates source focus once"
    );
    clear_pending_seams();
}

#[test]
fn stay_with_null_source_takes_no_setter() {
    let mut rig = rig(false, Direction::Right, &["mover"], &[], true);
    let correlation = CorrelationId::parse("corr-send-stay-null").expect("correlation");
    stage_success(&mut rig, &correlation);
    let outcome = run_send(&mut rig, &correlation, 34);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right", "mover transferred on stay");
    assert_eq!(
        pending_counters().focus,
        0,
        "null source MRU takes no focus setter"
    );
    clear_pending_seams();
}

#[test]
fn resolve_covers_four_directions_negative_stacked_and_panel_gaps() {
    use super::rmax03_adapter_tests::test_state as fresh_state;
    let mut state = fresh_state();
    state.workspaces.ensure_output("src");
    state.workspaces.ensure_output("east");
    state.workspaces.ensure_output("south");
    state.workspaces.ensure_output("west-neg");
    // Full rectangles edge-touch in all four directions while work areas
    // leave panel gaps: adjacency still selects (panels cannot block).
    let areas = vec![
        area(
            "src",
            full_rect(0, 40, 800, 1000),
            full_rect(0, 0, 800, 1040),
        ),
        area(
            "east",
            full_rect(800, 0, 800, 1080),
            full_rect(800, 0, 800, 1080),
        ),
        area(
            "south",
            full_rect(0, 1040, 800, 600),
            full_rect(0, 1040, 800, 600),
        ),
        area(
            "west-neg",
            full_rect(-800, 0, 800, 1080),
            full_rect(-800, 0, 800, 1080),
        ),
    ];
    for output in ["src", "east", "south", "west-neg"] {
        state.workspaces.ensure_output(output);
    }
    let mover = full_rect(100, 100, 200, 200);
    assert_eq!(
        resolve_output_send_target(&state, &areas, "src", Direction::Right, &mover)
            .expect("readable")
            .map(|(o, _)| o),
        Some("east".to_owned())
    );
    assert_eq!(
        resolve_output_send_target(&state, &areas, "src", Direction::Down, &mover)
            .expect("readable")
            .map(|(o, _)| o),
        Some("south".to_owned())
    );
    assert_eq!(
        resolve_output_send_target(&state, &areas, "src", Direction::Left, &mover)
            .expect("readable")
            .map(|(o, _)| o),
        Some("west-neg".to_owned())
    );
    assert_eq!(
        resolve_output_send_target(
            &state,
            &areas,
            "east",
            Direction::Left,
            &full_rect(850, 100, 200, 200)
        )
        .expect("readable")
        .map(|(o, _)| o),
        Some("src".to_owned())
    );
    // Up from src has no candidate: no-op, never wrap.
    assert_eq!(
        resolve_output_send_target(&state, &areas, "src", Direction::Up, &mover).expect("readable"),
        None
    );
    // Unknown source output refuses instead of skipping.
    assert_eq!(
        resolve_output_send_target(&state, &areas, "ghost", Direction::Right, &mover),
        Err("unreadable-topology")
    );
}

#[test]
fn non_left_top_target_resolves_and_pins_after_relocation() {
    // Stacked eastern candidates: the mover centre projection picks the
    // lower (non-top) output; the full path must deliver exactly there.
    let stacked = vec![
        area(
            "primary",
            full_rect(0, 40, 1920, 1040),
            full_rect(0, 0, 1920, 1080),
        ),
        area(
            "upper",
            full_rect(1920, 0, 1920, 540),
            full_rect(1920, 0, 1920, 540),
        ),
        area(
            "lower",
            full_rect(1920, 540, 1920, 540),
            full_rect(1920, 540, 1920, 540),
        ),
    ];
    let mut state = test_state();
    for output in ["primary", "upper", "lower"] {
        state.workspaces.ensure_output(output);
        let ws = state.workspaces.active_id(output).expect("active");
        state.workspaces.set_tiled(output, &ws, true);
    }
    let ws_lower = state.workspaces.active_id("lower").expect("active");
    let mover_rect = full_rect(100, 700, 400, 300);
    let resolved =
        resolve_output_send_target(&state, &stacked, "primary", Direction::Right, &mover_rect)
            .expect("readable")
            .expect("candidate");
    assert_eq!(resolved.0, "lower", "centre picks the non-top candidate");
    assert_eq!(resolved.1, ws_lower, "destination current view pinned");
    // A relocated mover re-resolves differently (no candidate looking
    // Right from the arrival): the committed path below pins the
    // pre-transfer resolution instead of re-ranking.
    let arrived = full_rect(2000, 600, 400, 300);
    assert_ne!(
        resolve_output_send_target(&state, &stacked, "lower", Direction::Right, &arrived)
            .expect("readable"),
        Some(resolved.clone()),
        "post-arrival re-resolution must not equal the pinned target"
    );
}

#[test]
fn late_topology_drift_retains_source_and_engine() {
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let correlation = CorrelationId::parse("corr-send-topology").expect("correlation");
    let mut drifted = rig.areas.clone();
    drifted[1].full.x = 2000;
    inject_monitor_areas(Some(drifted));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 35);
    assert_eq!(outcome, "send-unverified");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary");
    assert_eq!(loc.workspace, rig.home.workspace);
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "late topology drift keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "refused transfer skips focus");
    assert!(
        pending_counters().reconcile >= 1,
        "refused transfer converges with the fresh topology"
    );
    clear_pending_seams();
}

#[test]
fn stale_view_member_lifetime_and_scope_refuse_without_commit() {
    // Stale destination view: the pinned workspace is no longer current.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let correlation = CorrelationId::parse("corr-send-view").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        rig.target_ws = "ws-gone".to_owned();
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 36);
        assert_eq!(outcome, "view-changed");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "stale view keeps the retained Engine"
        );
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary", "stale view keeps source membership");
        clear_pending_seams();
    }
    // Moved membership: the mover no longer homes on the pinned source.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let correlation = CorrelationId::parse("corr-send-member").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        let ws_b = rig.target_ws.clone();
        assert!(
            rig.state
                .workspaces
                .assign(rig.mover.clone(), "right", &ws_b, false)
        );
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 37);
        assert_eq!(outcome, "origin-vanished");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "moved membership keeps the retained Engine"
        );
        clear_pending_seams();
    }
    // Lifetime drift: the live member tag no longer matches the stored tag.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let correlation = CorrelationId::parse("corr-send-lifetime").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        inject_pending_tag(rig.mover.hwnd, Some("tag-recycled".to_owned()));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 38);
        assert_eq!(outcome, "identity-changed");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "lifetime drift keeps the retained Engine"
        );
        assert!(
            !rig.state.member_tokens.contains_key(&rig.mover),
            "lifetime drift drops the stale membership"
        );
        clear_pending_seams();
    }
    // Scope drift: the mover executable leaves the explicit scope.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let correlation = CorrelationId::parse("corr-send-scope").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        rig.state.scope = vec!["other-app.exe".to_owned()];
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 39);
        assert_eq!(outcome, "scope-changed");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "scope drift keeps the retained Engine"
        );
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary", "scope drift keeps source membership");
        clear_pending_seams();
    }
}

#[test]
fn late_apply_and_arrival_failures_retain_engine_and_membership() {
    // Failed readback keeps source membership and the retained Engine.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let correlation = CorrelationId::parse("corr-send-readback").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_apply(Some((false, 0)));
        inject_pending_focus(Some("focus-ok"));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 40);
        assert_eq!(outcome, "send-unverified");
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "failed readback keeps the retained Engine"
        );
        assert_eq!(pending_counters().focus, 0, "failed write skips focus");
        clear_pending_seams();
    }
    // Arrival mismatch (stable still shows the source frame) refuses the
    // same way without commit, revert, or replay.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let correlation = CorrelationId::parse("corr-send-arrival").expect("correlation");
        let home_frame = rig.observed[0].visible;
        rig.state.stable.insert(rig.mover.hwnd, home_frame);
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 41);
        assert_eq!(outcome, "send-unverified");
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "arrival mismatch keeps the retained Engine"
        );
        clear_pending_seams();
    }
}

#[test]
fn focus_failure_after_commit_keeps_verified_transfer() {
    // Foundation semantics: a focus failure after a committed verified
    // transfer reports the focus outcome with the transfer retained.
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let correlation = CorrelationId::parse("corr-send-focus-fail").expect("correlation");
    stage_success(&mut rig, &correlation);
    inject_pending_focus(Some("focus-unverified"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 42);
    assert_eq!(outcome, "focus-unverified");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right", "verified transfer stays committed");
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "verified transfer still publishes the Engine"
    );
    clear_pending_seams();
}

#[test]
fn fullscreen_stay_takes_no_mover_setter() {
    use super::inject_pending_overlay_seq;
    // STAY never focuses the mover: a D16-admitted fullscreen float
    // transfers membership-only with no geometry, and the sole mover
    // empties the source, so the null source takes no setter at all.
    let mut rig = rig(false, Direction::Right, &["mover"], &[], true);
    stage_float_exception(&mut rig);
    rig.state
        .overlay_now
        .insert("mover".to_owned(), (false, true));
    {
        let mut hint_cx = HintCx::new();
        let corr = CorrelationId::parse("reassemble-full-stay").expect("c");
        let rows = super::assemble_domain_rows(
            &mut rig.state,
            "primary",
            &rig.home.workspace.clone(),
            &rig.observed.clone(),
            &[],
            "reassemble-full-stay",
            corr.as_str(),
            &mut hint_cx,
        )
        .expect("complete source rows");
        assert!(
            rows.iter().any(|r| r.token == "mover" && r.floating),
            "re-assembled mover row rides floating"
        );
        rig.source_rows = rows;
    }
    let correlation = CorrelationId::parse("corr-send-d16-full-stay").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![
        (false, Some(true)),
        (false, Some(true)),
        (false, Some(true)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_send(&mut rig, &correlation, 60);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(
        loc.output, "right",
        "deferred fullscreen stay still moves membership"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "null-source stay takes no focus setter, mover untouched"
    );
    assert!(
        rig.state.snap_advance.is_none(),
        "stay never advances focus state toward the mover"
    );
    clear_pending_seams();
}

#[test]
fn floating_target_boundary_is_membership_only() {
    let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
    let correlation = CorrelationId::parse("corr-send-float-target").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 43);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right", "floating boundary still transfers");
    assert!(
        !rig.state.workspaces.is_hidden(&rig.mover),
        "visible destination keeps the arrival visible"
    );
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "tiled source survivors still reflow through the candidate Engine"
    );
    assert_eq!(
        pending_counters().apply,
        1,
        "only the tiled source side reflows; the floating side takes no writes"
    );
    assert_eq!(
        pending_apply_writables(),
        vec![BTreeSet::from(["s1".to_owned()])],
        "source reflow scopes the survivor only: mover absent"
    );
    assert_eq!(pending_counters().focus, 1, "follow still focuses once");
    clear_pending_seams();
}

#[test]
fn floating_source_occupied_target_reflows_occupants_without_mover() {
    // Floating source with an occupied tiled target: the target side
    // reflows its occupants while the mover takes no geometry write.
    let mut rig = rig(true, Direction::Right, &["mover"], &["t1"], true);
    let ws_a = rig.home.workspace.clone();
    rig.state.workspaces.set_tiled("primary", &ws_a, false);
    let correlation =
        CorrelationId::parse("corr-send-native-target-writable").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_send(&mut rig, &correlation, 57);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right");
    assert_eq!(
        pending_apply_writables(),
        vec![BTreeSet::from(["t1".to_owned()])],
        "target reflow scopes existing occupants only: mover absent"
    );
    clear_pending_seams();
}

#[test]
fn native_after_apply_overlay_drift_refuses_without_publish() {
    use super::inject_pending_overlay_seq;
    // Post-reflow drift: the entry reads match the snapshot while the
    // final fresh read drifts, so the native transfer refuses after
    // geometry with both retained authorities standing and no focus.
    let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
    let correlation = CorrelationId::parse("corr-send-native-final-drift").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    // Reads on this path: run pre-fence, native entry gate/live (one
    // shared read), native final live. The drift entry lands on the
    // final gate.
    inject_pending_overlay_seq(vec![
        (false, Some(false)),
        (false, Some(false)),
        (true, Some(false)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 58);
    assert_eq!(outcome, "overlay-changed");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary", "post-reflow drift never publishes");
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "post-reflow drift keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "post-reflow drift skips focus");
    clear_pending_seams();
}

#[test]
fn native_stable_fullscreen_carry_focuses_mover() {
    use super::inject_pending_overlay_seq;
    // A D16-admitted fullscreen float on a floating boundary lifts the
    // fullscreen refusal through the deferred carry-through and transfers
    // membership-only; follow focuses the verified mover exactly like an
    // ordinary follow.
    let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
    stage_float_exception(&mut rig);
    rig.state
        .overlay_now
        .insert("mover".to_owned(), (false, true));
    {
        let mut hint_cx = HintCx::new();
        let corr = CorrelationId::parse("reassemble-native-full").expect("c");
        let rows = super::assemble_domain_rows(
            &mut rig.state,
            "primary",
            &rig.home.workspace.clone(),
            &rig.observed.clone(),
            &[],
            "reassemble-native-full",
            corr.as_str(),
            &mut hint_cx,
        )
        .expect("complete source rows");
        assert!(
            rows.iter().any(|r| r.token == "mover" && r.floating),
            "re-assembled mover row rides floating"
        );
        rig.source_rows = rows;
    }
    let correlation = CorrelationId::parse("corr-send-native-full").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    // Reads on this path: run pre-fence, native entry gate/live (one
    // shared read), native final live - all stable.
    inject_pending_overlay_seq(vec![
        (false, Some(true)),
        (false, Some(true)),
        (false, Some(true)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 59);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(
        loc.output, "right",
        "deferred fullscreen carry still moves membership"
    );
    assert_eq!(
        pending_counters().focus,
        1,
        "stable-fullscreen follow focuses the verified mover once"
    );
    // Setter identity and order: `send-applied` is only reachable past
    // the arrival gates, and the single actuation targeted the mover.
    assert_eq!(
        rig.state
            .snap_advance
            .as_ref()
            .map(|origin| (origin.token.clone(), origin.hwnd)),
        Some(("mover".to_owned(), rig.mover.hwnd)),
        "follow focus advances the snap origin to the verified mover"
    );
    assert_eq!(
        rig.state.workspaces.focus_target(
            "right",
            &rig.target_ws,
            &[rig.mover.clone()].into_iter().collect::<BTreeSet<_>>()
        ),
        Some(rig.mover.clone()),
        "follow advances the project MRU to the mover"
    );
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "deferred transfer still publishes the Engine"
    );
    clear_pending_seams();
}

/// Float the rig mover in its source Engine session without D16
/// admission (no `overlay_now` entry), mirroring the production
/// ToggleFloat converge path.
fn stage_float_exception(rig: &mut OutputSendRig) {
    use tiler_core::boundary::CoreCommand;
    let (domain, key) =
        workspace_domain_for(&rig.home.output, &rig.home.workspace, &rig.areas, 8, 8)
            .expect("source domain");
    let rows = vec![(
        WindowId("mover".to_owned()),
        Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        },
        tiler_core::size_hints::WindowSizeHints::none(),
        false,
    )];
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(id, rect, _, _)| (id.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let correlation = CorrelationId::parse("float-mover").expect("correlation");
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &rig.state.owner,
        &rig.state.generation,
        &correlation,
        rig.state
            .engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        fp,
        &domain,
        &key,
        8,
        &rows,
        Some(&WindowId("mover".to_owned())),
    );
    event.command = CoreCommand::ToggleFloat {
        window: "mover".to_owned(),
        float_rect: None,
    };
    let reply = rig.state.engine.handle(&event);
    assert!(
        !matches!(
            reply,
            tiler_core::boundary::CoreReply::Rejected { .. }
                | tiler_core::boundary::CoreReply::Diverged(_)
                | tiler_core::boundary::CoreReply::SnapshotInvalid { .. }
        ),
        "float toggle converges, got {reply:?}"
    );
}

#[test]
fn native_late_topology_drift_refuses_before_commit() {
    // Genuinely late drift: tick-time areas pass the dispatch fences
    // while the re-read fresh topology fails, so the native tail refuses
    // before any membership commit, reconciles, and retains both
    // authorities.
    let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
    let correlation = CorrelationId::parse("corr-send-native-topology").expect("correlation");
    let mut drifted = rig.areas.clone();
    drifted[1].full.x = 2000;
    inject_monitor_areas(Some(drifted));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 47);
    assert_eq!(outcome, "send-unverified");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary", "late drift keeps source membership");
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "late drift keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "refused transfer skips focus");
    assert!(
        pending_counters().reconcile >= 1,
        "refused transfer converges with the fresh topology"
    );
    clear_pending_seams();
}

#[test]
fn native_stale_view_and_lifetime_refuse_without_commit() {
    // Stale destination view on a floating boundary: the pinned
    // workspace is no longer current, so no membership stages.
    {
        let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
        let correlation = CorrelationId::parse("corr-send-native-view").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        rig.target_ws = "ws-gone".to_owned();
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 48);
        assert_eq!(outcome, "view-changed");
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary", "stale view keeps source membership");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "stale view keeps the retained Engine"
        );
        clear_pending_seams();
    }
    // Lifetime drift on a floating boundary: the live tag mismatch drops
    // the stale member with no commit.
    {
        let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
        let correlation = CorrelationId::parse("corr-send-native-lifetime").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_focus(Some("focus-ok"));
        inject_pending_tag(rig.mover.hwnd, Some("tag-recycled".to_owned()));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 49);
        assert_eq!(outcome, "identity-changed");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "lifetime drift keeps the retained Engine"
        );
        assert!(
            !rig.state.member_tokens.contains_key(&rig.mover),
            "lifetime drift drops the stale membership"
        );
        clear_pending_seams();
    }
}

#[test]
fn native_apply_failure_preserves_original_authorities() {
    // Source-side reflow refusal: the transfer lives only on discarded
    // candidates, so the original membership and Engine stand with no
    // assign-back needed.
    {
        let mut rig = rig(true, Direction::Right, &["mover", "s1"], &[], false);
        let correlation =
            CorrelationId::parse("corr-send-native-rollback-src").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_apply(Some((false, 0)));
        inject_pending_focus(Some("focus-ok"));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 50);
        assert_eq!(outcome, "send-unverified");
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(
            (loc.output.as_str(), loc.workspace.as_str()),
            ("primary", rig.home.workspace.as_str()),
            "refused reflow never stages membership on the target"
        );
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "refused reflow discards the candidate Engine"
        );
        assert_eq!(
            pending_counters().focus,
            0,
            "unpublished transfer skips focus"
        );
        clear_pending_seams();
    }
    // Target-side reflow refusal (floating source, tiled target): the
    // same preservation holds; the mover frame was never written.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let ws_a = rig.home.workspace.clone();
        rig.state.workspaces.set_tiled("primary", &ws_a, false);
        let correlation =
            CorrelationId::parse("corr-send-native-rollback-dst").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_apply(Some((false, 0)));
        inject_pending_focus(Some("focus-ok"));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 51);
        assert_eq!(outcome, "send-unverified");
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary", "refused target reflow never stages");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "refused target reflow discards the candidate Engine"
        );
        clear_pending_seams();
    }
    // Target-side mismatch reports distinctly, still without publishing.
    {
        let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
        let ws_a = rig.home.workspace.clone();
        rig.state.workspaces.set_tiled("primary", &ws_a, false);
        let correlation = CorrelationId::parse("corr-send-native-mismatch").expect("correlation");
        inject_monitor_areas(Some(rig.areas.clone()));
        inject_pending_observe(rig.observed.clone(), Vec::new(), false);
        inject_pending_apply(Some((true, 1)));
        inject_pending_focus(Some("focus-ok"));
        let engine_before = format!("{:?}", rig.state.engine);
        let outcome = run_send(&mut rig, &correlation, 52);
        assert_eq!(outcome, "send-mismatch");
        let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
        assert_eq!(loc.output, "primary", "mismatched reflow never stages");
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "mismatched reflow discards the candidate Engine"
        );
        clear_pending_seams();
    }
}

#[test]
fn floating_source_float_exception_is_excluded() {
    // Floating source plus an unadmitted float mover: excluded
    // independent of the workspace mode (decided contract).
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let ws_a = rig.home.workspace.clone();
    rig.state.workspaces.set_tiled("primary", &ws_a, false);
    stage_float_exception(&mut rig);
    let correlation = CorrelationId::parse("corr-send-float-source-refused").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 53);
    assert_eq!(outcome, "send-refused-floating");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary", "unadmitted float never transfers");
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "float refusal keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "float refusal never focuses");
    clear_pending_seams();
}

#[test]
fn admitted_float_maximized_sends_deferred_without_geometry() {
    use super::inject_pending_overlay_seq;
    // D16 happy path: an observed float mover carrying the exact live
    // overlay snapshot (maximized, not fullscreen) sends tiled-to-tiled
    // through the slotless deferred arm - membership-only, no geometry
    // application, follow focuses once.
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    stage_float_exception(&mut rig);
    rig.state
        .overlay_now
        .insert("mover".to_owned(), (true, false));
    // Re-assemble through production assembly: the float toggle postdates
    // the rig rows, and production always assembles post-toggle rows
    // (the mover row flips floating with the same frame).
    {
        let mut hint_cx = HintCx::new();
        let corr = CorrelationId::parse("reassemble").expect("c");
        let rows = super::assemble_domain_rows(
            &mut rig.state,
            "primary",
            &rig.home.workspace.clone(),
            &rig.observed.clone(),
            &[],
            "reassemble",
            corr.as_str(),
            &mut hint_cx,
        )
        .expect("complete source rows");
        assert!(
            rows.iter().any(|r| r.token == "mover" && r.floating),
            "re-assembled mover row rides floating"
        );
        rig.source_rows = rows;
    }
    let correlation = CorrelationId::parse("corr-send-d16-happy").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(true, Some(false)), (true, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 54);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(
        loc.output, "right",
        "deferred transfer still moves membership"
    );
    assert_eq!(
        pending_counters().apply,
        0,
        "slotless deferred transfer applies no geometry"
    );
    assert_eq!(pending_counters().focus, 1, "deferred follow focuses once");
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "deferred transfer still publishes the Engine"
    );
    clear_pending_seams();
}

#[test]
fn admitted_float_fullscreen_combo_sends_deferred_with_mover_focus() {
    use super::inject_pending_overlay_seq;
    // D16 fullscreen-combo happy path: an observed float mover carrying
    // the exact live overlay snapshot (not maximized, fullscreen) lifts
    // the fullscreen refusal through the deferred carry-through and
    // transfers membership-only with no geometry; follow focuses the
    // verified mover once, exactly like an ordinary follow.
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    stage_float_exception(&mut rig);
    rig.state
        .overlay_now
        .insert("mover".to_owned(), (false, true));
    {
        let mut hint_cx = HintCx::new();
        let corr = CorrelationId::parse("reassemble-full").expect("c");
        let rows = super::assemble_domain_rows(
            &mut rig.state,
            "primary",
            &rig.home.workspace.clone(),
            &rig.observed.clone(),
            &[],
            "reassemble-full",
            corr.as_str(),
            &mut hint_cx,
        )
        .expect("complete source rows");
        assert!(
            rows.iter().any(|r| r.token == "mover" && r.floating),
            "re-assembled mover row rides floating"
        );
        rig.source_rows = rows;
    }
    let correlation = CorrelationId::parse("corr-send-d16-full").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(true)), (false, Some(true))]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 56);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(
        loc.output, "right",
        "deferred fullscreen carry still moves membership"
    );
    assert_eq!(
        pending_counters().apply,
        0,
        "slotless deferred transfer applies no geometry"
    );
    assert_eq!(
        pending_counters().focus,
        1,
        "stable-fullscreen follow focuses the verified mover once"
    );
    // Setter identity and order: `send-applied` is only reachable past
    // the arrival gates, and the single actuation targeted the mover.
    assert_eq!(
        rig.state
            .snap_advance
            .as_ref()
            .map(|origin| (origin.token.clone(), origin.hwnd)),
        Some(("mover".to_owned(), rig.mover.hwnd)),
        "follow focus advances the snap origin to the verified mover"
    );
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "deferred transfer still publishes the Engine"
    );
    clear_pending_seams();
}

#[test]
fn final_overlay_drift_refuses_after_writes_without_commit() {
    use super::inject_pending_overlay_seq;
    // Post-write drift: the pre-write live read matches the snapshot
    // while the final live read drifts, so the Engine-route transfer
    // refuses after geometry with both authorities retained.
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let correlation = CorrelationId::parse("corr-send-final-drift").expect("correlation");
    let planned = predict_send_output_mover_rect(&rig, &correlation);
    assert_eq!(output_for_rect(&rig.areas, &planned), "right");
    rig.state.stable.insert(rig.mover.hwnd, planned);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (true, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 55);
    assert_eq!(outcome, "overlay-changed");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(
        loc.output, "primary",
        "post-write drift keeps source membership"
    );
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "post-write drift keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "post-write drift skips focus");
    clear_pending_seams();
}

#[test]
fn floating_source_tiled_mover_stays_eligible() {
    // An originally tiled member of a floating workspace sends through
    // the native boundary path; sticky and float-exception subjects below
    // stay excluded. The floating source takes no reflow while the tiled
    // target reflows around the preserved mover frame.
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let ws_a = rig.home.workspace.clone();
    rig.state.workspaces.set_tiled("primary", &ws_a, false);
    let correlation = CorrelationId::parse("corr-send-float-source").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 44);
    assert_eq!(outcome, "send-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right");
    assert_eq!(
        pending_counters().apply,
        1,
        "only the tiled target side reflows; the floating source takes no writes"
    );
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "verified target reflow publishes the candidate Engine with the staged membership"
    );
    // The mover exclusion from the target writable set is structural
    // (one `writable.remove` beside the apply); the stubbed native apply
    // cannot observe per-window writes headless, so frame preservation
    // rests on that line plus the membership-only contract above.
    clear_pending_seams();
}

#[test]
fn sticky_mover_is_excluded() {
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    let correlation = CorrelationId::parse("corr-send-sticky").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    rig.state.sticky.insert(rig.mover.clone(), true);
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &correlation, 45);
    assert_eq!(outcome, "send-refused-sticky");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary", "sticky never transfers");
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "sticky refusal keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "sticky refusal never focuses");
    clear_pending_seams();
}

#[test]
fn unadmitted_float_mover_is_excluded() {
    let mut rig = rig(true, Direction::Right, &["mover"], &[], true);
    // Float the mover in its source Engine session without D16 admission
    // (no `overlay_now` entry): the subject gate must refuse.
    stage_float_exception(&mut rig);
    let send_correlation = CorrelationId::parse("corr-send-float").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_send(&mut rig, &send_correlation, 46);
    assert_eq!(outcome, "send-refused-floating");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary", "unadmitted float never transfers");
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "float refusal keeps the retained Engine"
    );
    clear_pending_seams();
}
