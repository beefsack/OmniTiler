#![forbid(unsafe_code)]
//! G-06 maximized directional focus fence + G-D2 maximized workspace-send
//! carry (portable, headless).
//!
//! Drives the real `session::Session` focus/send APIs plus one real
//! `engine::Engine` focus request. G-06: with the Linux-route
//! `maximized_focus_fence` opt-in on, directional focus with a maximized
//! (non-fullscreen) focused subject refuses as `Unchanged` with no plan and
//! no pending; fullscreen keeps its enter/leave behavior and the default
//! (Windows) route is unchanged. G-D2: a maximized tiled mover sends to
//! another workspace through the ordinary transfer with its leaf preserved
//! and follow/stay focus intact (no unmaximize/remaximize in core).

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::contract::{
    AckOutcome, AdapterAck, FocusCapabilities, LifecycleCapabilities, LifecycleOperation,
    Observation,
};
use crate::directional::{Capabilities, Direction, OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionObservation,
};
use std::collections::BTreeMap;

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
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
/// Complete observation over the retained membership with `overrides`
/// applied per window id (used to flag the focused mover maximized).
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
fn maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.maximized = true;
    entry
}
fn fullscreen_maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = true;
    entry.maximized = true;
    entry
}
fn linux_session(domains: Vec<OutputDomain>) -> Session {
    let mut s = Session::new(owner(), generation(), 0, 7, domains).expect("session");
    // Linux route: fixed-size admission plus the G-06 focus fence.
    s.set_fixed_size_admission(true);
    s.set_maximized_focus_fence(true);
    s
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

/// Two-tile session focused on win-2 (win-1 left of win-2).
fn two_tile() -> (Session, DomainKey) {
    let mut s = linux_session(vec![dom("out-1", "ws-1")]);
    admit(&mut s, "win-1", "out-1", "ws-1", "corr-admit-1");
    admit(&mut s, "win-2", "out-1", "ws-1", "corr-admit-2");
    let domain = key("out-1", "ws-1");
    assert_eq!(focused_window(&s, &domain).0, "win-2");
    (s, domain)
}

#[test]
fn fence_defaults_off_preserving_the_windows_route() {
    let engine = Engine::new();
    assert!(!engine.fixed_size_admission(), "fixed opt-in defaults off");
    assert!(
        !engine.maximized_focus_fence(),
        "G-06 fence defaults off: Windows carriers keep exact current behavior"
    );
    let s = Session::new(owner(), generation(), 0, 7, vec![dom("out-1", "ws-1")]).expect("s");
    assert!(!s.maximized_focus_fence(), "session fence defaults off");
}

#[test]
fn fence_on_refuses_maximized_local_focus_with_no_pending() {
    let (mut s, domain) = two_tile();
    let focused = focused_window(&s, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs(&s, &flags);
    let err = s
        .propose_focus(
            &domain,
            &focused,
            Direction::Left,
            &o0,
            &corr("corr-g06-focus"),
            &FocusCapabilities::full(),
        )
        .expect_err("maximized focus must fence");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!s.has_pending(), "fence stages no pending");
    assert!(!s.has_pending_desired(), "fence stages no desired state");
}

#[test]
fn fence_on_fullscreen_skips_local_sibling_while_ordinary_focus_plans() {
    let (mut s, domain) = two_tile();
    let focused = focused_window(&s, &domain);
    // Fullscreen (even with maximize also set) skips same-output siblings
    // (R-MAX-02): the local propose refuses as `Unchanged` with no plan and
    // no pending; the cross leg continues to the adjacent output instead
    // (covered in `fullscreen_focus_cross_tests`).
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        fullscreen_maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs(&s, &flags);
    let err = s
        .propose_focus(
            &domain,
            &focused,
            Direction::Left,
            &o0,
            &corr("corr-g06-fs"),
            &FocusCapabilities::full(),
        )
        .expect_err("fullscreen focus skips the local sibling");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!s.has_pending(), "fence stages no pending");
    assert!(!s.has_pending_desired(), "fence stages no desired state");
    // Ordinary focus still plans under the fence.
    let o1 = obs(&s, &BTreeMap::new());
    s.propose_focus(
        &domain,
        &focused,
        Direction::Left,
        &o1,
        &corr("corr-g06-plain"),
        &FocusCapabilities::full(),
    )
    .expect("ordinary focus still plans");
}

#[test]
fn fence_off_plans_maximized_focus_preserving_windows_behavior() {
    // Fence off with fixed admission on: the maximized observation binds
    // and focus plans exactly as before (Windows route unchanged).
    let mut s = Session::new(owner(), generation(), 0, 7, vec![dom("out-1", "ws-1")]).expect("s");
    s.set_fixed_size_admission(true);
    assert!(!s.maximized_focus_fence());
    admit(&mut s, "win-1", "out-1", "ws-1", "corr-admit-1");
    admit(&mut s, "win-2", "out-1", "ws-1", "corr-admit-2");
    let domain = key("out-1", "ws-1");
    let focused = focused_window(&s, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs(&s, &flags);
    s.propose_focus(
        &domain,
        &focused,
        Direction::Left,
        &o0,
        &corr("corr-g06-off"),
        &FocusCapabilities::full(),
    )
    .expect("fence off plans maximized focus as before");
}

#[test]
fn fence_on_refuses_maximized_cross_output_focus() {
    let mut s = linux_session(vec![
        dom_adjacent("out-1", "ws-1", Direction::Right, "out-2"),
        dom_adjacent("out-2", "ws-1", Direction::Left, "out-1"),
    ]);
    admit(&mut s, "win-t", "out-2", "ws-1", "corr-admit-t");
    admit(&mut s, "win-a", "out-1", "ws-1", "corr-admit-a");
    let source = key("out-1", "ws-1");
    assert_eq!(focused_window(&s, &source).0, "win-a");
    // Maximized subject fences the cross-output leg (no plan, no pending).
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-a".to_owned(),
        maximized_flag(&tiled("win-a", "out-1", "ws-1")),
    );
    let o1 = obs(&s, &flags);
    let err = s
        .propose_cross_output_focus(
            &source,
            &WindowId("win-a".to_owned()),
            Direction::Right,
            &o1,
            &corr("corr-g06-cross"),
            &FocusCapabilities::full(),
        )
        .expect_err("maximized cross-output focus must fence");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!s.has_pending(), "fence stages no pending");
    // Control: ordinary cross-output focus plans (sole source leaf at the
    // root edge crosses to the remembered target leaf).
    let o0 = obs(&s, &BTreeMap::new());
    s.propose_cross_output_focus(
        &source,
        &WindowId("win-a".to_owned()),
        Direction::Right,
        &o0,
        &corr("corr-g06-cross-plain"),
        &FocusCapabilities::full(),
    )
    .expect("ordinary cross-output focus plans");
}

#[test]
fn fence_on_leaves_maximized_move_untouched_for_the_adapter_clear() {
    // The Engine never clears maximize itself: a maximized mover still
    // plans structurally, so the KDE adapter's unmaximize-first leg lands
    // on the ordinary move once settlement is observed clear.
    let (mut s, domain) = two_tile();
    let focused = focused_window(&s, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs(&s, &flags);
    let _ = s
        .propose_move(
            &domain,
            &focused,
            Direction::Left,
            &o0,
            &corr("corr-g06-move"),
            &Capabilities::full(),
        )
        .expect("move still plans under the focus-only fence")
        .expect_moved("tiled move");
}

/// Occupied-target session focused on win-2 (win-t waits on ws-b).
fn send_fixture() -> (Session, DomainKey, DomainKey) {
    let mut s = linux_session(vec![dom("out-1", "ws-a"), dom("out-1", "ws-b")]);
    admit(&mut s, "win-t", "out-1", "ws-b", "corr-admit-t");
    admit(&mut s, "win-1", "out-1", "ws-a", "corr-admit-1");
    admit(&mut s, "win-2", "out-1", "ws-a", "corr-admit-2");
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    assert_eq!(focused_window(&s, &source).0, "win-2");
    (s, source, target)
}

fn mover_leaf(s: &Session, window: &str) -> crate::directional::NodeId {
    s.snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == window)
        .expect("mover link")
        .leaf
        .clone()
}

#[test]
fn maximized_mover_sends_with_leaf_preserved_and_follow_focus() {
    let (mut s, _source, target) = send_fixture();
    let leaf = mover_leaf(&s, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        maximized_flag(&tiled("win-2", "out-1", "ws-a")),
    );
    let o0 = obs(&s, &flags);
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-2".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: true,
            },
            &o0,
            &corr("corr-gd2-follow"),
            &LifecycleCapabilities::full(),
        )
        .expect("maximized send plans the ordinary transfer");
    // The whole window (leaf identity) relocates; core performs no
    // unmaximize/remaximize.
    match &plan.dispatch.operation {
        LifecycleOperation::MoveTiled {
            window,
            leaf: operation_leaf,
            target_output,
            target_workspace,
            ..
        } => {
            assert_eq!(window.0, "win-2");
            assert_eq!(
                operation_leaf, &leaf,
                "mover keeps its leaf identity across the send"
            );
            assert_eq!(target_output, &target.output);
            assert_eq!(target_workspace, &target.workspace);
        }
        other => panic!("send must plan MoveTiled, got {other:?}"),
    }
    assert_eq!(
        plan.desired_focus_domain,
        Some(target.clone()),
        "follow moves focus into the target"
    );
    assert_eq!(
        plan.desired_focus_leaf,
        Some(leaf),
        "follow keeps the mover leaf"
    );
    assert!(
        plan.desired_geometry
            .iter()
            .any(|g| g.window.0 == "win-2" && g.workspace == target.workspace),
        "mover keeps a retained target allocation (overlay, never removed)"
    );
    assert!(
        plan.desired_geometry.iter().any(|g| g.window.0 == "win-t"),
        "occupied target geometry covers the existing tile"
    );
}

#[test]
fn maximized_mover_stay_falls_back_to_source_mru() {
    let (mut s, source, target) = send_fixture();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        maximized_flag(&tiled("win-2", "out-1", "ws-a")),
    );
    let o0 = obs(&s, &flags);
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-2".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: false,
            },
            &o0,
            &corr("corr-gd2-stay"),
            &LifecycleCapabilities::full(),
        )
        .expect("maximized stay-send plans");
    assert_eq!(
        plan.desired_focus_domain,
        Some(source.clone()),
        "stay keeps the source selected"
    );
    let survivor = plan.desired_focus_leaf.clone().expect("stay survivor leaf");
    let survivor_window = plan
        .desired_snapshot
        .windows
        .iter()
        .find(|l| {
            l.leaf == survivor && l.output == source.output && l.workspace == source.workspace
        })
        .expect("stay survivor link")
        .window
        .0
        .clone();
    assert_eq!(
        survivor_window, "win-1",
        "stay falls back to the source MRU survivor"
    );
}

// ---- Real-Engine request: Linux-route opt-ins propagate and the focus
// fence refuses through `Engine::handle`. ----

fn engine_window(id: &str, x: i32, maximized: bool) -> EngineWindow {
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
        fit_excluded: maximized,
        fullscreen: false,
        maximized,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
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

fn seed_engine(fence: bool) -> Engine {
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    // Linux route: fixed admission plus the G-06 fence. Windows carriers
    // use `Engine::new` defaults (both off).
    engine.set_fixed_size_admission(true);
    engine.set_maximized_focus_fence(fence);
    // Seed two side-by-side tiles focused on win-2 via reconcile.
    let seed = focus_event(
        vec![
            engine_window("win-1", 0, false),
            engine_window("win-2", 200, false),
        ],
        "win-2",
        "corr-gd2-seed",
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
fn engine_fence_on_refuses_maximized_focus_through_handle() {
    let mut engine = seed_engine(true);
    let event = focus_event(
        vec![
            engine_window("win-1", 0, false),
            engine_window("win-2", 200, true),
        ],
        "win-2",
        "corr-g06-engine",
        7,
    );
    match engine.handle(&event) {
        CoreReply::Rejected { kind, .. } => {
            assert_eq!(kind, "unchanged", "fence refuses as unchanged")
        }
        other => panic!("maximized focus must fence, got {other:?}"),
    }
}

#[test]
fn engine_fence_off_plans_maximized_focus_preserving_windows_behavior() {
    let mut engine = seed_engine(false);
    let event = focus_event(
        vec![
            engine_window("win-1", 0, false),
            engine_window("win-2", 200, true),
        ],
        "win-2",
        "corr-g06-engine-off",
        7,
    );
    match engine.handle(&event) {
        CoreReply::FocusDirectional(_) => {}
        other => panic!("fence off must plan maximized focus as before, got {other:?}"),
    }
}

#[test]
fn engine_fence_on_fullscreen_skips_local_while_ordinary_plans() {
    let mut engine = seed_engine(true);
    let ordinary = focus_event(
        vec![
            engine_window("win-1", 0, false),
            engine_window("win-2", 200, false),
        ],
        "win-2",
        "corr-g06-engine-plain",
        7,
    );
    match engine.handle(&ordinary) {
        CoreReply::FocusDirectional(_) => {}
        other => panic!("ordinary focus must plan under the fence, got {other:?}"),
    }
    // Fullscreen skips the local sibling (R-MAX-02): the local request
    // refuses as unchanged; the cross leg continues instead.
    let mut full = engine_window("win-2", 200, true);
    full.fullscreen = true;
    let event = focus_event(
        vec![engine_window("win-1", 0, false), full],
        "win-2",
        "corr-g06-engine-fs",
        7,
    );
    match engine.handle(&event) {
        CoreReply::Rejected { kind, .. } => {
            assert_eq!(kind, "unchanged", "fullscreen skips local")
        }
        other => panic!("fullscreen focus must skip the local sibling, got {other:?}"),
    }
}
