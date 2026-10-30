#![forbid(unsafe_code)]
//! R-MAX-02: while the focused window is fullscreen, directional focus
//! skips all same-output siblings and continues to the adjacent output in
//! that direction; without one, focus stays. Explicit focus-by-window,
//! workspace switches and directional move/send are unaffected.
//!
//! Failing-first regressions: fullscreen tiled/floating subjects must not
//! take same-output siblings (Session `propose_focus` refuses `Unchanged`
//! in every direction; the Session cross leg skips the local-exhaustion
//! gate; the shared WorldCommand entry skips the float lattice and marks
//! the cross command float-origin when the subject floats).

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::contract::{
    AckOutcome, AdapterAck, FocusCapabilities, FocusPostObservation, LifecycleCapabilities,
    Observation,
};
use crate::directional::{Direction, OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionObservation,
};
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    EffectKind, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};
use std::collections::BTreeMap;

// ---- Session fixtures (mirrors maximized_focus_send_tests.rs). ----

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn corr(v: &str) -> CorrelationId {
    CorrelationId::parse(v).expect("valid")
}
fn dom(o: &str, w: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn dom_adjacent(o: &str, w: &str, dir: Direction, target: &str) -> OutputDomain {
    let mut d = dom(o, w);
    d.adjacent.insert(dir, OutputId(target.to_owned()));
    d
}
fn key(o: &str, w: &str) -> DomainKey {
    DomainKey {
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
    }
}
fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}
fn fullscreen_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = true;
    entry
}
fn fullscreen_maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = true;
    entry.maximized = true;
    entry
}
fn obs(s: &Session, overrides: &BTreeMap<String, ObservedWindow>) -> SessionObservation {
    let mut wins: Vec<ObservedWindow> = s
        .snapshot()
        .windows
        .iter()
        .map(|l| {
            if let Some(entry) = overrides.get(&l.window.0) {
                entry.clone()
            } else {
                tiled(&l.window.0, &l.output.0, &l.workspace.0)
            }
        })
        .collect();
    wins.extend(s.exception_observed());
    SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 100),
        windows: wins,
    }
}
fn admit(s: &mut Session, w: &str, o: &str, ws: &str, c: &str) {
    let o0 = obs(s, &BTreeMap::new());
    let mut with_new = o0.windows.clone();
    with_new.push(tiled(w, o, ws));
    let o0 = SessionObservation {
        observation: o0.observation,
        windows: with_new,
    };
    let cmd = SessionCommand::Admit {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        },
        suppress_fixed_float: false,
    };
    let plan = s
        .propose(&cmd, &o0, &corr(c), &LifecycleCapabilities::full())
        .expect("admit");
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
}
fn focused_window(s: &Session, domain: &DomainKey) -> WindowId {
    let (d, l) = s.focus();
    let d = d.expect("focused domain");
    let l = l.expect("focused leaf");
    assert_eq!(&d, domain);
    s.snapshot()
        .windows
        .iter()
        .find(|w| w.leaf == l && w.output == domain.output && w.workspace == domain.workspace)
        .expect("focused link")
        .window
        .clone()
}
fn commit_focus(s: &mut Session, plan: &crate::session::SessionFocusPlan, c: &str) {
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    let post = FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 950 + base),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    let commit = s.verify_focus(&post).expect("verify focus");
    assert_eq!(commit.revision, base + 1);
}

/// Two tiles on one output, focused on win-2 (win-1 left of win-2).
/// Fixed-size admission is on so the fullscreen overlay observation binds
/// (same lane as the maximized-fence fixtures).
fn two_tile() -> (Session, DomainKey) {
    let mut s = Session::new(owner(), generation(), 0, 7, vec![dom("out-1", "ws-1")]).expect("s");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-1", "out-1", "ws-1", "corr-admit-1");
    admit(&mut s, "win-2", "out-1", "ws-1", "corr-admit-2");
    let domain = key("out-1", "ws-1");
    assert_eq!(focused_window(&s, &domain).0, "win-2");
    (s, domain)
}

/// Two adjacent outputs. Fixed-size admission is on for the fullscreen
/// overlay binding (see `two_tile`).
fn two_output() -> Session {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            dom_adjacent("out-1", "ws-1", Direction::Right, "out-2"),
            dom_adjacent("out-2", "ws-1", Direction::Left, "out-1"),
        ],
    )
    .expect("s");
    s.set_fixed_size_admission(true);
    s
}

#[test]
fn fullscreen_tiled_subject_propose_focus_refuses_unchanged_all_directions() {
    let (mut s, domain) = two_tile();
    let focused = focused_window(&s, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        fullscreen_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs(&s, &flags);
    // A local sibling waits in Left; Up/Down have no local target either.
    // Every direction must refuse as Unchanged (no plan, no pending) so the
    // caller continues to the adjacent output or stays.
    for direction in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        let err = s
            .propose_focus(
                &domain,
                &focused,
                direction,
                &o0,
                &corr("corr-rmax02-focus"),
                &FocusCapabilities::full(),
            )
            .expect_err("fullscreen focus must skip same-output siblings");
        assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
        assert!(!s.has_pending(), "fence stages no pending");
        assert!(!s.has_pending_desired(), "fence stages no desired state");
    }
}

#[test]
fn non_fullscreen_control_still_plans_local_sibling() {
    let (mut s, domain) = two_tile();
    let focused = focused_window(&s, &domain);
    let o0 = obs(&s, &BTreeMap::new());
    s.propose_focus(
        &domain,
        &focused,
        Direction::Left,
        &o0,
        &corr("corr-rmax02-control"),
        &FocusCapabilities::full(),
    )
    .expect("ordinary focus still plans the local sibling");
}

#[test]
fn fullscreen_tiled_subject_crosses_to_adjacent_output() {
    let mut s = two_output();
    admit(&mut s, "win-t", "out-2", "ws-1", "corr-admit-t");
    admit(&mut s, "win-a", "out-1", "ws-1", "corr-admit-a");
    admit(&mut s, "win-b", "out-1", "ws-1", "corr-admit-b");
    let source = key("out-1", "ws-1");
    assert_eq!(focused_window(&s, &source).0, "win-b");
    // Move logical focus onto win-a so a same-output sibling (win-b) waits
    // in the press direction.
    let plain = obs(&s, &BTreeMap::new());
    let plan = s
        .propose_focus(
            &source,
            &WindowId("win-b".to_owned()),
            Direction::Left,
            &plain,
            &corr("corr-rmax02-seed"),
            &FocusCapabilities::full(),
        )
        .expect("seed focus plans");
    commit_focus(&mut s, &plan, "corr-rmax02-seed");
    assert_eq!(focused_window(&s, &source).0, "win-a");
    // Fullscreen win-a pressing Right must skip win-b and cross to win-t.
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-a".to_owned(),
        fullscreen_flag(&tiled("win-a", "out-1", "ws-1")),
    );
    let o1 = obs(&s, &flags);
    let plan = s
        .propose_cross_output_focus(
            &source,
            &WindowId("win-a".to_owned()),
            Direction::Right,
            &o1,
            &corr("corr-rmax02-cross"),
            &FocusCapabilities::full(),
        )
        .expect("fullscreen subject crosses despite the local sibling");
    assert_eq!(plan.desired_focus_domain, key("out-2", "ws-1"));
    let target_window = plan
        .desired_snapshot
        .windows
        .iter()
        .find(|l| {
            l.leaf == plan.desired_focus_leaf
                && l.output == key("out-2", "ws-1").output
                && l.workspace == key("out-2", "ws-1").workspace
        })
        .expect("cross target link")
        .window
        .clone();
    assert_eq!(target_window.0, "win-t");
}

#[test]
fn fullscreen_single_output_cross_refuses_without_pending() {
    let (mut s, domain) = two_tile();
    // Focus win-1 so the press direction holds a sibling; there is still no
    // adjacent output to continue to.
    let plain = obs(&s, &BTreeMap::new());
    let focused = focused_window(&s, &domain);
    let plan = s
        .propose_focus(
            &domain,
            &focused,
            Direction::Left,
            &plain,
            &corr("corr-rmax02-seed2"),
            &FocusCapabilities::full(),
        )
        .expect("seed focus plans");
    commit_focus(&mut s, &plan, "corr-rmax02-seed2");
    assert_eq!(focused_window(&s, &domain).0, "win-1");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-1".to_owned(),
        fullscreen_flag(&tiled("win-1", "out-1", "ws-1")),
    );
    let o1 = obs(&s, &flags);
    let err = s
        .propose_cross_output_focus(
            &domain,
            &WindowId("win-1".to_owned()),
            Direction::Right,
            &o1,
            &corr("corr-rmax02-noadj"),
            &FocusCapabilities::full(),
        )
        .expect_err("no adjacent output must retain");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!s.has_pending(), "no pending on retain");
}

#[test]
fn vertical_fullscreen_cross_refuses() {
    let (mut s, domain) = two_tile();
    let focused = focused_window(&s, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        fullscreen_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs(&s, &flags);
    // Left/Right-only crossings stay: Up/Down fullscreen is a no-op.
    for direction in [Direction::Up, Direction::Down] {
        let err = s
            .propose_cross_output_focus(
                &domain,
                &focused,
                direction,
                &o0,
                &corr("corr-rmax02-vert"),
                &FocusCapabilities::full(),
            )
            .expect_err("vertical fullscreen must not cross");
        assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    }
}

#[test]
fn both_flags_follow_fullscreen_not_maximized_fence() {
    // Maximized (non-fullscreen) fences; fullscreen (even with maximize
    // also set) follows the R-MAX-02 cross rule instead.
    let mut s = two_output();
    s.set_maximized_focus_fence(true);
    admit(&mut s, "win-t", "out-2", "ws-1", "corr-admit-t");
    admit(&mut s, "win-a", "out-1", "ws-1", "corr-admit-a");
    admit(&mut s, "win-b", "out-1", "ws-1", "corr-admit-b");
    let source = key("out-1", "ws-1");
    let plain = obs(&s, &BTreeMap::new());
    let plan = s
        .propose_focus(
            &source,
            &WindowId("win-b".to_owned()),
            Direction::Left,
            &plain,
            &corr("corr-rmax02-seed3"),
            &FocusCapabilities::full(),
        )
        .expect("seed focus plans");
    commit_focus(&mut s, &plan, "corr-rmax02-seed3");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-a".to_owned(),
        fullscreen_maximized_flag(&tiled("win-a", "out-1", "ws-1")),
    );
    let o1 = obs(&s, &flags);
    // Local propose refuses Unchanged (fullscreen rule, all directions)...
    let err = s
        .propose_focus(
            &source,
            &WindowId("win-a".to_owned()),
            Direction::Right,
            &o1,
            &corr("corr-rmax02-both"),
            &FocusCapabilities::full(),
        )
        .expect_err("both flags must not take the local sibling");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    // ...while the cross leg proceeds (a maximized non-fullscreen subject
    // would fence here instead).
    let plan = s
        .propose_cross_output_focus(
            &source,
            &WindowId("win-a".to_owned()),
            Direction::Right,
            &o1,
            &corr("corr-rmax02-both-cross"),
            &FocusCapabilities::full(),
        )
        .expect("both flags cross like fullscreen");
    assert_eq!(plan.desired_focus_domain, key("out-2", "ws-1"));
}

// ---- Raw Engine: local fullscreen focus refuses through `handle`. ----

fn engine_window(id: &str, x: i32, fullscreen: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: Rect {
            x,
            y: 0,
            w: 100,
            h: 100,
        },
        floating: false,
        fit_excluded: false,
        fullscreen,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn focus_event(
    windows: Vec<EngineWindow>,
    focused: &str,
    correlation: &str,
    fingerprint: u64,
) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr(correlation),
        revision: 0,
        fingerprint,
        domain: dom("out-1", "ws-1"),
        domain_key: key("out-1", "ws-1"),
        outer_gap: 0,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Focus {
            window: focused.to_owned(),
            direction: "left".to_owned(),
            cross_output_transfer: false,
            float_subject: false,
        },
    }
}

fn seed_engine() -> Engine {
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    // Fixed-size admission so the fullscreen overlay observation binds
    // (same lane as the maximized-fence engine fixtures).
    engine.set_fixed_size_admission(true);
    let seed = focus_event(
        vec![
            engine_window("win-1", 0, false),
            engine_window("win-2", 200, false),
        ],
        "win-2",
        "corr-rmax02-seed",
        7,
    );
    let seed = CoreEvent {
        command: CoreCommand::Reconcile,
        ..seed
    };
    match engine.handle(&seed) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("seed must project, got {other:?}"),
    }
    engine
}

#[test]
fn engine_fullscreen_local_focus_refuses_unchanged() {
    let mut engine = seed_engine();
    let event = focus_event(
        vec![
            engine_window("win-1", 0, false),
            engine_window("win-2", 200, true),
        ],
        "win-2",
        "corr-rmax02-engine",
        7,
    );
    match engine.handle(&event) {
        CoreReply::Rejected { kind, .. } => {
            assert_eq!(kind, "unchanged", "fullscreen local focus refuses")
        }
        other => panic!("fullscreen focus must not plan locally, got {other:?}"),
    }
}

// ---- Shared WorldCommand entry (WorldEngine). ----

fn wrect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(wrect(x, 0, 800, 600)),
        work_area: Some(wrect(x, 0, 800, 560)),
        gap: 8,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(wrect(x, 0, 40, 90)),
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
    }
}

fn fullscreen_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: true,
        ..fact(window, output, x)
    }
}

fn float_fact(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
    WindowFact {
        floating: true,
        rect: Some(wrect(x, y, 200, 200)),
        ..fact(window, output, x)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::default(),
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        born_maximized: crate::born_maximized::BornMaximized::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
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
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn one_screen() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true)]
}

fn focus_targets(reply: &crate::world_report::WorldReply) -> Vec<WindowId> {
    reply
        .effects
        .iter()
        .filter(|e| e.kind == EffectKind::Focus)
        .map(|e| e.target.clone())
        .collect()
}

fn home_of(engine: &WorldEngine, window: &WindowId) -> Option<DomainKey> {
    for key in engine.engine.domain_keys() {
        if let Some(session) = engine.engine.session(&key)
            && session
                .snapshot()
                .windows
                .iter()
                .any(|l| &l.window == window)
        {
            return Some(key);
        }
    }
    None
}

fn session_focused(engine: &WorldEngine, key: &DomainKey) -> Option<WindowId> {
    let session = engine.engine.session(key)?;
    let (_, leaf) = session.focus();
    let leaf = leaf?;
    session.focused_window_for(&leaf, key)
}

#[test]
fn world_fullscreen_tiled_skips_sibling_and_crosses_output() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    // Fullscreen win-a pressing Right must skip win-b and cross to win-c.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fullscreen_fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-c".to_owned())],
        "fullscreen focus crosses to the adjacent output, never the sibling"
    );
}

#[test]
fn world_non_fullscreen_control_plans_local_sibling() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "ordinary focus still takes the same-output sibling"
    );
}

#[test]
fn world_fullscreen_single_output_retains_without_effect() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let outcome = engine.handle_report(&report(
        2,
        one_screen(),
        vec![
            fullscreen_fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 200),
        ],
        "win-a",
        Some(11),
        Some(WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
        }),
    ));
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(
                kind, "unchanged",
                "single-output fullscreen refuses as unchanged, not {kind}"
            );
            assert_eq!(
                session_focused(&engine, &key),
                Some(WindowId("win-a".to_owned())),
                "single-output fullscreen retains focus"
            );
            return;
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert!(
        focus_targets(&reply).is_empty(),
        "no focus effect without an adjacent output, got {:?}",
        reply.effects
    );
    assert_eq!(
        session_focused(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "single-output fullscreen retains focus"
    );
}

#[test]
fn world_fullscreen_vertical_is_noop() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let outcome = engine.handle_report(&report(
        2,
        two_screens(),
        vec![
            fullscreen_fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 200),
            fact("win-c", "out-b", 900),
        ],
        "win-a",
        Some(11),
        Some(WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
        }),
    ));
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(
                kind, "unchanged",
                "vertical fullscreen refuses as unchanged, not {kind}"
            );
            assert_eq!(
                session_focused(&engine, &key),
                Some(WindowId("win-a".to_owned())),
                "vertical fullscreen retains focus"
            );
            return;
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert!(
        focus_targets(&reply).is_empty(),
        "vertical fullscreen never crosses, got {:?}",
        reply.effects
    );
    assert_eq!(
        session_focused(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "vertical fullscreen retains focus"
    );
}

#[test]
fn world_fullscreen_float_skips_lattice_and_crosses_remembered() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // Establish the remembered tiled target on out-b, then return explicit
    // focus to the float subject.
    accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-t",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-t".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-sub".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    // Fullscreen subject pressing Right must skip win-east and cross to the
    // remembered win-t.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                WindowFact {
                    fullscreen: true,
                    ..float_fact("win-sub", "out-a", 100, 100)
                },
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            Some(13),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-t".to_owned())],
        "fullscreen float crosses to the remembered target, never the sibling"
    );
}

#[test]
fn world_fullscreen_float_vertical_retains() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-above", "out-a", 100, 0),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // A same-output float waits above, but the fullscreen subject pressing
    // Up must retain without dispatch.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            one_screen(),
            vec![
                WindowFact {
                    fullscreen: true,
                    ..float_fact("win-sub", "out-a", 100, 100)
                },
                float_fact("win-above", "out-a", 100, 0),
            ],
            "win-sub",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: "up".to_owned(),
            }),
        ),
    );
    let outcome = reply.command_result.as_ref().map(|result| match result {
        crate::world_report::CommandResult::Directional { outcome, .. } => outcome.as_str(),
        _ => "other",
    });
    assert_eq!(outcome, Some("focus-float-retained"));
    assert!(
        focus_targets(&reply).is_empty(),
        "vertical fullscreen float retains, got {:?}",
        reply.effects
    );
}

#[test]
fn world_fullscreen_float_single_output_retains() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // A same-output float waits to the right, but with no adjacent output
    // the fullscreen subject pressing Right must retain without dispatch.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            one_screen(),
            vec![
                WindowFact {
                    fullscreen: true,
                    ..float_fact("win-sub", "out-a", 100, 100)
                },
                float_fact("win-east", "out-a", 300, 100),
            ],
            "win-sub",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    let outcome = reply.command_result.as_ref().map(|result| match result {
        crate::world_report::CommandResult::Directional { outcome, .. } => outcome.as_str(),
        _ => "other",
    });
    assert_eq!(outcome, Some("focus-float-retained"));
    assert!(
        focus_targets(&reply).is_empty(),
        "single-output fullscreen float retains, got {:?}",
        reply.effects
    );
}

#[test]
fn world_explicit_focus_by_window_works_under_fullscreen() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    // Explicit focus-by-window is unaffected by the fullscreen subject.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fullscreen_fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "explicit focus still reaches the sibling under fullscreen"
    );
}
