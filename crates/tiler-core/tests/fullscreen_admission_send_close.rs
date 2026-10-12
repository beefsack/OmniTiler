//! D01 native activation ownership (admission over an overlay) + D28
//! fullscreen workspace-send carry + CLOSE-05 overlay removal (portable,
//! headless).
//!
//! Drives the real `session::Session` convergence/send/remove primitives:
//! - D01: admitting C while B is fullscreen/maximized retains B's slot
//!   (leaf identity) and follows the observed native focus: host-denied
//!   (focus stays B) keeps B focused, host-approved (focus moves to C)
//!   focuses C. The adapter-side focus fence (no actuation while an
//!   overlay is involved) is covered in the KWin script tests.
//! - D28: an explicit `MoveToWorkspace` of a retained fullscreen tiled
//!   mover plans the ordinary `MoveTiled` transfer with its leaf
//!   preserved and follow/stay focus intact (no unfullscreen in core),
//!   mirroring the G-D2 maximized carry.
//! - CLOSE-05: removing a fullscreen/maximized focused B refills survivor
//!   A and focuses A.

use std::collections::BTreeMap;

use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecycleOperation, LifecyclePostObservation,
    Observation,
};
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, Session, SessionCommand,
    SessionObservation, SessionPlan,
};

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
            w: 120,
            h: 80,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
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
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}
fn flagged(base: &ObservedWindow, fullscreen: bool, maximized: bool) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = fullscreen;
    entry.maximized = maximized;
    entry
}
/// Complete observation over the retained membership plus `extra`,
/// applying per-window flag overrides by id.
fn obs(
    s: &Session,
    extra: Vec<ObservedWindow>,
    overrides: &BTreeMap<String, ObservedWindow>,
) -> SessionObservation {
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
    wins.extend(extra);
    SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 100),
        windows: wins,
    }
}
fn admit(s: &mut Session, w: &str, o: &str, ws: &str, c: &str) {
    let o0 = obs(s, vec![tiled(w, o, ws)], &BTreeMap::new());
    let cmd = SessionCommand::Admit {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: Rect {
            x: 0,
            y: 0,
            w: 60,
            h: 40,
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
    s.verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
}
fn leaf_of(s: &Session, window: &str) -> NodeId {
    s.snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == window)
        .expect("link")
        .leaf
        .clone()
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
fn commit_plan(s: &mut Session, plan: &SessionPlan, c: &str) {
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    let post = LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200 + base),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    s.verify_lifecycle(&post).expect("commit");
}

/// Two-tile session focused on win-2 (win-1 left of win-2).
fn two_tile() -> (Session, DomainKey) {
    let mut s = Session::new(owner(), generation(), 0, 7, vec![dom("out-1", "ws-1")]).expect("s");
    // Fixed-size admission opt-in: overlay observations (fullscreen /
    // maximized) bind advisory-style instead of failing legacy binding,
    // with no effect on ordinary hint-less admissions.
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-1", "out-1", "ws-1", "corr-admit-1");
    admit(&mut s, "win-2", "out-1", "ws-1", "corr-admit-2");
    let domain = key("out-1", "ws-1");
    assert_eq!(focused_window(&s, &domain).0, "win-2");
    (s, domain)
}

#[test]
fn admission_over_fullscreen_denied_keeps_slot_and_focus() {
    // Host denied activation: C arrives while native focus stays on
    // fullscreen B. B keeps its slot and focus.
    let (mut s, domain) = two_tile();
    let leaf_b = leaf_of(&s, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), true, false),
    );
    let o0 = obs(&s, vec![tiled("win-c", "out-1", "ws-1")], &flags);
    let counts = s
        .converge_observation(&o0, Some(&WindowId("win-2".to_owned())))
        .expect("admission converges");
    assert_eq!(counts.admitted, 1);
    assert_eq!(leaf_of(&s, "win-2"), leaf_b, "B retains its tile slot");
    assert_eq!(
        focused_window(&s, &domain).0,
        "win-2",
        "denied C keeps B focused"
    );
    assert!(
        s.snapshot().windows.iter().any(|l| l.window.0 == "win-c"),
        "C is structurally admitted behind the overlay"
    );
}

#[test]
fn admission_over_fullscreen_approved_keeps_slot_moves_focus() {
    // Host approved activation: C arrives with native focus on C. B keeps
    // its slot while focus follows the host to the newcomer.
    let (mut s, domain) = two_tile();
    let leaf_b = leaf_of(&s, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), true, false),
    );
    let o0 = obs(&s, vec![tiled("win-c", "out-1", "ws-1")], &flags);
    let counts = s
        .converge_observation(&o0, Some(&WindowId("win-c".to_owned())))
        .expect("admission converges");
    assert_eq!(counts.admitted, 1);
    assert_eq!(leaf_of(&s, "win-2"), leaf_b, "B retains its tile slot");
    assert_eq!(
        focused_window(&s, &domain).0,
        "win-c",
        "approved C takes focus"
    );
}

#[test]
fn admission_over_maximized_denied_keeps_slot_and_focus() {
    // Maximized native activation ownership: same structural rule as the
    // fullscreen leg with the maximized overlay.
    let (mut s, domain) = two_tile();
    let leaf_b = leaf_of(&s, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), false, true),
    );
    let o0 = obs(&s, vec![tiled("win-c", "out-1", "ws-1")], &flags);
    let counts = s
        .converge_observation(&o0, Some(&WindowId("win-2".to_owned())))
        .expect("admission converges");
    assert_eq!(counts.admitted, 1);
    assert_eq!(leaf_of(&s, "win-2"), leaf_b, "B retains its tile slot");
    assert_eq!(
        focused_window(&s, &domain).0,
        "win-2",
        "denied C keeps B focused"
    );
}

/// Occupied-target session focused on win-2 (win-t waits on ws-b).
fn send_fixture() -> (Session, DomainKey, DomainKey) {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![dom("out-1", "ws-a"), dom("out-1", "ws-b")],
    )
    .expect("s");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-t", "out-1", "ws-b", "corr-admit-t");
    admit(&mut s, "win-1", "out-1", "ws-a", "corr-admit-1");
    admit(&mut s, "win-2", "out-1", "ws-a", "corr-admit-2");
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    assert_eq!(focused_window(&s, &source).0, "win-2");
    (s, source, target)
}

#[test]
fn fullscreen_mover_sends_with_leaf_preserved_and_follow_focus() {
    // D28: an explicit send of a retained fullscreen tiled mover plans the
    // ordinary transfer with its leaf preserved and follow focus intact;
    // core performs no unfullscreen.
    let (mut s, _source, target) = send_fixture();
    let leaf = leaf_of(&s, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-a"), true, false),
    );
    let o0 = obs(&s, vec![], &flags);
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-2".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: true,
            },
            &o0,
            &corr("corr-d28-follow"),
            &LifecycleCapabilities::full(),
        )
        .expect("fullscreen send plans the ordinary transfer");
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
fn fullscreen_mover_stay_falls_back_to_source_mru() {
    let (mut s, source, target) = send_fixture();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-a"), true, false),
    );
    let o0 = obs(&s, vec![], &flags);
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-2".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: false,
            },
            &o0,
            &corr("corr-d28-stay"),
            &LifecycleCapabilities::full(),
        )
        .expect("fullscreen stay-send plans");
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

fn remove_and_commit_flags(
    s: &mut Session,
    window: &str,
    flags: &BTreeMap<String, ObservedWindow>,
    c: &str,
) -> SessionPlan {
    let o0 = obs(s, vec![], flags);
    let plan = s
        .propose(
            &SessionCommand::Remove {
                window: WindowId(window.to_owned()),
            },
            &o0,
            &corr(c),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("remove {window} proposes: {e:?}"));
    commit_plan(s, &plan, c);
    plan
}

#[test]
fn close_fullscreen_removal_refills_survivor_and_focus() {
    // CLOSE-05 fullscreen leg: closing focused fullscreen B removes the
    // tile, survivor A refills the exact domain rect, focus falls to A.
    let (mut s, domain) = two_tile();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), true, false),
    );
    let plan = remove_and_commit_flags(&mut s, "win-2", &flags, "corr-close-fs");
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "win-2"),
        "removed overlay takes no allocation"
    );
    assert_eq!(
        plan.desired_geometry.len(),
        1,
        "remove plan covers exactly the survivor"
    );
    let survivor = plan
        .desired_geometry
        .iter()
        .find(|g| g.window.0 == "win-1")
        .expect("survivor geometry");
    assert_eq!(
        survivor.rect,
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80
        },
        "sole survivor refills the exact domain rect"
    );
    assert_eq!(
        plan.desired_focus_domain,
        Some(domain.clone()),
        "remove plan focuses the domain"
    );
    assert_eq!(
        plan.desired_focus_leaf,
        Some(leaf_of(&s, "win-1")),
        "remove plan focuses the survivor leaf"
    );
    assert!(
        s.snapshot().windows.iter().all(|l| l.window.0 != "win-2"),
        "overlay is gone with no restore"
    );
    assert_eq!(
        focused_window(&s, &domain).0,
        "win-1",
        "sole survivor takes focus"
    );
    assert_eq!(
        s.snapshot().windows.len(),
        1,
        "survivor alone refills the domain"
    );
}

#[test]
fn close_maximized_removal_refills_survivor_and_focus() {
    // CLOSE-05 maximized leg: same removal/refill/focus rule.
    let (mut s, domain) = two_tile();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), false, true),
    );
    let plan = remove_and_commit_flags(&mut s, "win-2", &flags, "corr-close-max");
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "win-2"),
        "removed overlay takes no allocation"
    );
    assert_eq!(
        plan.desired_geometry.len(),
        1,
        "remove plan covers exactly the survivor"
    );
    let survivor = plan
        .desired_geometry
        .iter()
        .find(|g| g.window.0 == "win-1")
        .expect("survivor geometry");
    assert_eq!(
        survivor.rect,
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80
        },
        "sole survivor refills the exact domain rect"
    );
    assert_eq!(
        plan.desired_focus_domain,
        Some(domain.clone()),
        "remove plan focuses the domain"
    );
    assert_eq!(
        plan.desired_focus_leaf,
        Some(leaf_of(&s, "win-1")),
        "remove plan focuses the survivor leaf"
    );
    assert!(
        s.snapshot().windows.iter().all(|l| l.window.0 != "win-2"),
        "overlay is gone with no restore"
    );
    assert_eq!(
        focused_window(&s, &domain).0,
        "win-1",
        "sole survivor takes focus"
    );
    assert_eq!(
        s.snapshot().windows.len(),
        1,
        "survivor alone refills the domain"
    );
}
