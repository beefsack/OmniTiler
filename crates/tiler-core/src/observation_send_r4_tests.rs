#![forbid(unsafe_code)]
//! Step-3 send/R4 immediate-commit rows (test-only, authorized).
//!
//! Design: docs/changes/observation-convergence-step-3.md. The Engine must
//! commit a planned send/R4 topology synchronously into the canonical
//! per-domain sessions (retaining survivor trees) and return both-domain
//! geometry plus the native assignment. Failed native moves converge on the
//! next complete observations with no phantom. Pre-implementation these fail:
//! send reseeds from spatial observation; R4 does not commit cross-output.

use std::collections::BTreeMap;

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::contract::{AckOutcome, AdapterAck, LifecycleCapabilities, Observation};
use crate::directional::{Direction, Node, OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, OutputDomain, Session, SessionCommand};

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn corr(v: &str) -> CorrelationId {
    CorrelationId::parse(v).expect("valid")
}
fn bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }
}
fn domain(output: &str, workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: bounds(),
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn pair_domain(output: &str, workspace: &str, neighbor: (Direction, &str)) -> OutputDomain {
    let mut d = domain(output, workspace);
    d.adjacent
        .insert(neighbor.0, OutputId(neighbor.1.to_owned()));
    d
}
fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}
fn carried(window: &str, output: &str, workspace: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x,
            y: 0,
            w: 10,
            h: 10,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn admit_to(s: &mut Session, window: &str, output: &str, workspace: &str, c: &str) {
    let mut windows: Vec<crate::session::ObservedWindow> = s
        .snapshot()
        .windows
        .iter()
        .map(|l| crate::session::ObservedWindow {
            window: l.window.clone(),
            output: l.output.clone(),
            workspace: l.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        })
        .collect();
    windows.extend(s.exception_observed());
    windows.push(crate::session::ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    });
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let base = s.accepted_revision();
    let obs = crate::session::SessionObservation {
        observation: Observation::new(owner(), generation(), base, 100 + base),
        windows,
    };
    let plan = s
        .propose(
            &SessionCommand::Admit {
                window: WindowId(window.to_owned()),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: bounds(),
                suppress_fixed_float: false,
            },
            &obs,
            &corr(c),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200 + base),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
}
fn leaves_of(s: &Session, o: &str, ws: &str) -> Vec<String> {
    let view = s
        .snapshot()
        .domains
        .iter()
        .find(|d| d.output.0 == o && d.workspace.0 == ws)
        .unwrap_or_else(|| panic!("domain {o}/{ws}"))
        .clone();
    fn go(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Leaf { id } => out.push(id.0.clone()),
            Node::Group { children, .. } => children.iter().for_each(|c| go(c, out)),
        }
    }
    match &view.tree {
        None => vec![],
        Some(t) => {
            let mut v = vec![];
            go(t, &mut v);
            v
        }
    }
}
fn homed(s: &Session, w: &str) -> Option<(String, String)> {
    s.snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == w)
        .map(|l| (l.output.0.clone(), l.workspace.0.clone()))
}
fn seed_send_engine() -> (Engine, OutputDomain, OutputDomain) {
    let mut source =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-a")]).expect("source");
    admit_to(&mut source, "win-s1", "out-1", "ws-a", "s-1");
    admit_to(&mut source, "win-s2", "out-1", "ws-a", "s-2");
    admit_to(&mut source, "win-m", "out-1", "ws-a", "s-3");
    let mut target =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-b")]).expect("target");
    admit_to(&mut target, "win-t1", "out-1", "ws-b", "t-1");
    admit_to(&mut target, "win-t2", "out-1", "ws-b", "t-2");
    let source_domain = source.domains().first().expect("sd").clone();
    let target_domain = target.domains().first().expect("td").clone();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(key("out-1", "ws-a"), source, 0);
    engine.store_committed(key("out-1", "ws-b"), target, 0);
    (engine, source_domain, target_domain)
}
fn send_event(source: &OutputDomain, target: &OutputDomain, c: &str) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr(c),
        revision: 0,
        fingerprint: 7,
        domain: source.clone(),
        domain_key: source.key(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        // Scrambled rects: spatial reseed would order survivors as
        // [s2, s1] / [t2, t1]; retained topology must stay [s1, s2] / [t1, t2].
        windows: vec![
            carried("win-s2", "out-1", "ws-a", 0),
            carried("win-s1", "out-1", "ws-a", 200),
            carried("win-m", "out-1", "ws-a", 400),
        ],
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target.clone(), target.key())),
        target_windows: vec![
            carried("win-t2", "out-1", "ws-b", 0),
            carried("win-t1", "out-1", "ws-b", 400),
        ],
        closed_windows: vec![],
        command: CoreCommand::SendToWorkspace {
            window: "win-m".to_owned(),
            target_output: "out-1".to_owned(),
            target_workspace: "ws-b".to_owned(),
            follow: true,
        },
    }
}
fn reconcile_event(domain_state: &OutputDomain, windows: Vec<EngineWindow>, c: &str) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr(c),
        revision: 0,
        fingerprint: 999,
        domain: domain_state.clone(),
        domain_key: domain_state.key(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Reconcile,
    }
}

#[test]
fn send_commits_immediately_with_retained_topology_and_stays() {
    let (mut engine, source_domain, target_domain) = seed_send_engine();
    let src_base = engine
        .session(&key("out-1", "ws-a"))
        .expect("src")
        .accepted_revision();
    let tgt_base = engine
        .session(&key("out-1", "ws-b"))
        .expect("tgt")
        .accepted_revision();
    let geometry = match engine.handle(&send_event(&source_domain, &target_domain, "send-1")) {
        CoreReply::SendWorkspace(plan) => plan.geometry,
        CoreReply::Tiled(plan) => plan.geometry,
        other => panic!("send must return a committed plan, got {other:?}"),
    };
    // Retained survivor order preserved (not spatial-reseeded), mover moved.
    let src = engine.session(&key("out-1", "ws-a")).expect("src retained");
    let tgt = engine.session(&key("out-1", "ws-b")).expect("tgt retained");
    assert_eq!(
        leaves_of(src, "out-1", "ws-a"),
        vec!["leaf-win-s1", "leaf-win-s2"]
    );
    assert_eq!(
        leaves_of(tgt, "out-1", "ws-b"),
        vec!["leaf-win-t1", "leaf-win-t2", "leaf-win-m"]
    );
    assert_eq!(homed(src, "win-m"), None);
    assert_eq!(
        homed(tgt, "win-m"),
        Some(("out-1".to_owned(), "ws-b".to_owned()))
    );
    assert!(src.accepted_revision() > src_base);
    assert!(tgt.accepted_revision() > tgt_base);
    // Both-domain geometry plus the native assignment.
    let mut ids: Vec<String> = geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["win-m", "win-s1", "win-s2", "win-t1", "win-t2"]);
    assert!(
        geometry
            .iter()
            .any(|g| g.window.0 == "win-m" && g.output.0 == "out-1" && g.workspace.0 == "ws-b")
    );
    // Native arrival stays: complete observations preserve both topologies.
    let src_obs = vec![
        carried("win-s1", "out-1", "ws-a", 0),
        carried("win-s2", "out-1", "ws-a", 10),
    ];
    let tgt_obs = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
        carried("win-m", "out-1", "ws-b", 20),
    ];
    match engine.handle(&reconcile_event(&source_domain, src_obs, "rec-s")) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("source reconcile must project, got {other:?}"),
    }
    match engine.handle(&reconcile_event(&target_domain, tgt_obs, "rec-t")) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("target reconcile must project, got {other:?}"),
    }
    let src = engine.session(&key("out-1", "ws-a")).expect("src");
    let tgt = engine.session(&key("out-1", "ws-b")).expect("tgt");
    assert_eq!(
        leaves_of(src, "out-1", "ws-a"),
        vec!["leaf-win-s1", "leaf-win-s2"]
    );
    assert_eq!(
        leaves_of(tgt, "out-1", "ws-b"),
        vec!["leaf-win-t1", "leaf-win-t2", "leaf-win-m"]
    );
}

#[test]
fn send_stay_keeps_source_selected_with_mru_focus() {
    let (mut engine, source_domain, target_domain) = seed_send_engine();
    let mut event = send_event(&source_domain, &target_domain, "send-stay-1");
    if let CoreCommand::SendToWorkspace { follow, .. } = &mut event.command {
        *follow = false;
    }
    let plan = match engine.handle(&event) {
        CoreReply::SendWorkspace(plan) => plan,
        other => panic!("stay must commit SendWorkspace, got {other:?}"),
    };
    assert!(!plan.follow);
    // Source stays selected: the MRU survivor keeps focus, never the target.
    assert_eq!(plan.focus_domain, Some(key("out-1", "ws-a")));
    assert_eq!(
        plan.focus_leaf.as_ref().map(|leaf| leaf.0.as_str()),
        Some("leaf-win-s2")
    );
    // Destination admission is unchanged from the follow path.
    let src = engine.session(&key("out-1", "ws-a")).expect("src retained");
    let tgt = engine.session(&key("out-1", "ws-b")).expect("tgt retained");
    assert_eq!(
        leaves_of(src, "out-1", "ws-a"),
        vec!["leaf-win-s1", "leaf-win-s2"]
    );
    assert_eq!(
        leaves_of(tgt, "out-1", "ws-b"),
        vec!["leaf-win-t1", "leaf-win-t2", "leaf-win-m"]
    );
    assert_eq!(
        homed(tgt, "win-m"),
        Some(("out-1".to_owned(), "ws-b".to_owned()))
    );
    assert_eq!(
        src.focus(),
        (
            Some(key("out-1", "ws-a")),
            Some(crate::directional::NodeId("leaf-win-s2".to_owned()))
        )
    );
    // Both-domain geometry with the native assignment on the target.
    let mut ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["win-m", "win-s1", "win-s2", "win-t1", "win-t2"]);
    assert!(
        plan.geometry
            .iter()
            .any(|g| g.window.0 == "win-m" && g.output.0 == "out-1" && g.workspace.0 == "ws-b")
    );
}

#[test]
fn failed_native_send_converges_to_source_without_phantom() {
    let (mut engine, source_domain, target_domain) = seed_send_engine();
    match engine.handle(&send_event(&source_domain, &target_domain, "send-1")) {
        CoreReply::SendWorkspace(_) | CoreReply::Tiled(_) => {}
        other => panic!("send must return a committed plan, got {other:?}"),
    }
    assert_eq!(
        homed(
            engine.session(&key("out-1", "ws-b")).expect("target"),
            "win-m"
        ),
        Some(("out-1".to_owned(), "ws-b".to_owned())),
        "native failure must follow an actual provisional Engine commit"
    );
    // Native refused: mover still observed on source, target without mover.
    let src_obs = vec![
        carried("win-s1", "out-1", "ws-a", 0),
        carried("win-s2", "out-1", "ws-a", 10),
        carried("win-m", "out-1", "ws-a", 20),
    ];
    let tgt_obs = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
    ];
    match engine.handle(&reconcile_event(&source_domain, src_obs, "rec-s")) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("source reconcile must project, got {other:?}"),
    }
    match engine.handle(&reconcile_event(&target_domain, tgt_obs, "rec-t")) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("target reconcile must project, got {other:?}"),
    }
    let src = engine.session(&key("out-1", "ws-a")).expect("src");
    let tgt = engine.session(&key("out-1", "ws-b")).expect("tgt");
    // Provisional target leaf withdrawn; mover admitted normally on source.
    assert_eq!(
        homed(src, "win-m"),
        Some(("out-1".to_owned(), "ws-a".to_owned()))
    );
    assert_eq!(homed(tgt, "win-m"), None);
    assert_eq!(
        leaves_of(src, "out-1", "ws-a"),
        vec!["leaf-win-s1", "leaf-win-s2", "leaf-win-m"]
    );
    assert!(
        !leaves_of(tgt, "out-1", "ws-b").contains(&"leaf-win-m".to_owned()),
        "no retained phantom on target"
    );
    for window in ["win-s1", "win-s2", "win-m", "win-t1", "win-t2"] {
        assert_eq!(
            usize::from(homed(src, window).is_some()) + usize::from(homed(tgt, window).is_some()),
            1
        );
    }
}

#[test]
fn r4_cross_output_commits_immediately_with_empty_target() {
    // Source out-1/ws-a holds H[win-a, win-m] with win-m focused at the right
    // edge; target out-2/ws-b is empty (no retained session).
    let mut source =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-a")]).expect("source");
    admit_to(&mut source, "win-a", "out-1", "ws-a", "r-a");
    admit_to(&mut source, "win-m", "out-1", "ws-a", "r-m");
    let source_pair = pair_domain("out-1", "ws-a", (Direction::Right, "out-2"));
    let target_pair = pair_domain("out-2", "ws-b", (Direction::Left, "out-1"));
    let (source_key, target_key) = (source_pair.key(), target_pair.key());
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(key("out-1", "ws-a"), source, 0);
    let event = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr("r4-1"),
        revision: 0,
        fingerprint: 999,
        domain: source_pair.clone(),
        domain_key: source_key.clone(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        windows: vec![
            carried("win-a", "out-1", "ws-a", 0),
            carried("win-m", "out-1", "ws-a", 10),
        ],
        directional: Some(vec![
            (source_pair.clone(), source_key.clone()),
            (target_pair.clone(), target_key.clone()),
        ]),
        directional_target_outer_gap: Some(0),
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Move {
            window: "win-m".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
        },
    };
    let geometry = match engine.handle(&event) {
        CoreReply::MoveDirectional(plan) => {
            assert_eq!(plan.rule, crate::directional::Rule::R4);
            plan.geometry
        }
        other => panic!("R4 must plan cross-output, got {other:?}"),
    };
    let src = engine.session(&source_key).expect("source retained");
    let tgt = engine.session(&target_key).expect("target retained");
    assert_eq!(leaves_of(src, "out-1", "ws-a"), vec!["leaf-win-a"]);
    assert_eq!(leaves_of(tgt, "out-2", "ws-b"), vec!["leaf-win-m"]);
    assert_eq!(
        homed(tgt, "win-m"),
        Some(("out-2".to_owned(), "ws-b".to_owned()))
    );
    let mut ids: Vec<String> = geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["win-a", "win-m"]);
    assert!(geometry.iter().all(|g| g.rect.w > 0 && g.rect.h > 0));
}

#[test]
fn r4_exhausted_target_floats_mover_as_send_output() {
    // Source out-1/ws-a holds H[win-a, win-m] with win-m focused at the
    // right edge; target out-2/ws-b is an exhausted 1x1 holding win-t. The
    // exact R4 wrap proves geometric exhaustion under the default float
    // result, so the Engine commits the shared capacity-float fallback
    // synchronously and returns the established send result (never a
    // refusal, never a redirect): the mover floats slotless at the chosen
    // target, the source releases, and the destination tree is untouched.
    let mut source =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-a")]).expect("source");
    admit_to(&mut source, "win-a", "out-1", "ws-a", "r-a");
    admit_to(&mut source, "win-m", "out-1", "ws-a", "r-m");
    let tiny_target = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-b".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: [(Direction::Left, OutputId("out-1".to_owned()))]
            .into_iter()
            .collect(),
    };
    // Stored components carry no adjacency (canonical form); the event pair
    // domains above carry the reciprocal fence.
    let tiny_component = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-b".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let mut target =
        Session::new(owner(), generation(), 0, 7, vec![tiny_component]).expect("target");
    admit_to(&mut target, "win-t", "out-2", "ws-b", "r-t");
    let mut source_pair = domain("out-1", "ws-a");
    source_pair
        .adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    let target_pair = tiny_target.clone();
    let (source_key, target_key) = (source_pair.key(), target_pair.key());
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(key("out-1", "ws-a"), source, 0);
    engine.store_committed(target_key.clone(), target, 0);
    let event = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr("r4-float-1"),
        revision: 0,
        fingerprint: 999,
        domain: source_pair.clone(),
        domain_key: source_key.clone(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        windows: vec![
            carried("win-a", "out-1", "ws-a", 0),
            carried("win-m", "out-1", "ws-a", 10),
            carried("win-t", "out-2", "ws-b", 0),
        ],
        directional: Some(vec![
            (source_pair.clone(), source_key.clone()),
            (target_pair.clone(), target_key.clone()),
        ]),
        directional_target_outer_gap: Some(0),
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Move {
            window: "win-m".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
        },
    };
    let geometry = match engine.handle(&event) {
        CoreReply::SendOutput(plan) => {
            match &plan.operation {
                crate::contract::LifecycleOperation::MoveToFloat {
                    window,
                    target_output,
                    target_workspace,
                    ..
                } => {
                    assert_eq!(window.0, "win-m");
                    assert_eq!(target_output.0, "out-2");
                    assert_eq!(target_workspace.0, "ws-b");
                }
                other => panic!("exhausted R4 floats, got {other:?}"),
            }
            assert!(plan.follow, "R4 always follows the mover");
            plan.geometry
        }
        other => panic!("exhausted R4 returns the send fallback, got {other:?}"),
    };
    let mut ids: Vec<String> = geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["win-a", "win-t"],
        "survivors project, mover slotless"
    );
    let src = engine.session(&source_key).expect("source retained");
    let tgt = engine.session(&target_key).expect("target retained");
    assert_eq!(leaves_of(src, "out-1", "ws-a"), vec!["leaf-win-a"]);
    assert_eq!(leaves_of(tgt, "out-2", "ws-b"), vec!["leaf-win-t"]);
    assert!(tgt.is_capacity_float(&WindowId("win-m".to_owned())));
    assert_eq!(homed(src, "win-m"), None, "source released");
}

#[test]
fn rapid_second_send_with_arrival_commits_without_reconcile() {
    // Step-3 Watch risk probe, arrival-showing case: first send A->B commits;
    // then, with NO intervening per-domain reconcile but native observations
    // already showing the arrival (m on B), a second send B->C (empty C)
    // commits immediately with the native assignment. No refusal, no
    // pending-exists/abandon wait.
    let (mut engine, source_domain, target_domain) = seed_send_engine();
    match engine.handle(&send_event(&source_domain, &target_domain, "send-1")) {
        CoreReply::SendWorkspace(_) | CoreReply::Tiled(_) => {}
        other => panic!("first send must commit, got {other:?}"),
    }
    assert_eq!(
        homed(engine.session(&key("out-1", "ws-b")).expect("tgt"), "win-m"),
        Some(("out-1".to_owned(), "ws-b".to_owned()))
    );
    let source_b = target_domain.clone();
    let target_c = domain("out-1", "ws-c");
    let b_windows = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
        carried("win-m", "out-1", "ws-b", 20),
    ];
    let second = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr("send-2"),
        revision: 0,
        fingerprint: 7,
        domain: source_b.clone(),
        domain_key: source_b.key(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        windows: b_windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target_c.clone(), target_c.key())),
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::SendToWorkspace {
            window: "win-m".to_owned(),
            target_output: "out-1".to_owned(),
            target_workspace: "ws-c".to_owned(),
            follow: true,
        },
    };
    match engine.handle(&second) {
        CoreReply::SendWorkspace(plan) => {
            assert!(
                plan.geometry.iter().any(|g| g.window.0 == "win-m"
                    && g.output.0 == "out-1"
                    && g.workspace.0 == "ws-c"),
                "second send must carry the native assignment, got {:?}",
                plan.geometry
            );
            assert_eq!(
                homed(
                    engine.session(&key("out-1", "ws-c")).expect("ws-c"),
                    "win-m"
                ),
                Some(("out-1".to_owned(), "ws-c".to_owned()))
            );
        }
        other => panic!("PROBE-RESULT rapid second send refused: {other:?}"),
    }
}

#[test]
fn stale_rapid_second_send_drops_mover_until_domain_reconcile() {
    // Step-3 Watch risk, stale-observation case: after A->B commits, a rapid
    // B->C send of win-m itself carries a STALE B observation (m absent,
    // still natively on A) before any per-domain reconcile. Pair convergence
    // drops the provisional m from the canonical sessions (stored before the
    // focus-sync refusal), so the second send refuses with focus-mismatch
    // and m is retained NOWHERE until the domains ordinary-reconcile. The
    // refusal clears on complete observations and the command is NOT lost
    // silently indefinitely: reconciling A (m present) re-admits m and a
    // retried A->C send commits.
    let (mut engine, source_domain, target_domain) = seed_send_engine();
    match engine.handle(&send_event(&source_domain, &target_domain, "send-1")) {
        CoreReply::SendWorkspace(_) | CoreReply::Tiled(_) => {}
        other => panic!("first send must commit, got {other:?}"),
    }
    let source_b = target_domain.clone();
    let target_c = domain("out-1", "ws-c");
    let stale_b_windows = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
    ];
    let second = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr("send-2"),
        revision: 0,
        fingerprint: 7,
        domain: source_b.clone(),
        domain_key: source_b.key(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        windows: stale_b_windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target_c.clone(), target_c.key())),
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::SendToWorkspace {
            window: "win-m".to_owned(),
            target_output: "out-1".to_owned(),
            target_workspace: "ws-c".to_owned(),
            follow: true,
        },
    };
    match engine.handle(&second) {
        CoreReply::Rejected { kind, .. } => assert_eq!(
            kind, "focus-mismatch",
            "stale rapid second send must refuse at focus sync"
        ),
        other => panic!("stale rapid second send must refuse, got {other:?}"),
    }
    // The converged removal was stored before the refusal: m is retained
    // nowhere, even though it is still natively on A.
    assert_eq!(
        engine
            .session(&key("out-1", "ws-b"))
            .map(|s| homed(s, "win-m"))
            .unwrap_or(None),
        None,
        "stale convergence drops provisional m from B"
    );
    assert_eq!(
        engine
            .session(&key("out-1", "ws-a"))
            .map(|s| homed(s, "win-m"))
            .unwrap_or(None),
        None,
        "m is retained nowhere until domain reconcile"
    );
    // Ordinary per-domain reconciles recover: A re-admits m normally.
    let a_obs = vec![
        carried("win-s1", "out-1", "ws-a", 0),
        carried("win-s2", "out-1", "ws-a", 10),
        carried("win-m", "out-1", "ws-a", 20),
    ];
    let b_obs = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
    ];
    match engine.handle(&reconcile_event(&source_domain, a_obs, "rec-a")) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("A reconcile must project, got {other:?}"),
    }
    match engine.handle(&reconcile_event(&target_domain, b_obs, "rec-b")) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("B reconcile must project, got {other:?}"),
    }
    assert_eq!(
        homed(engine.session(&key("out-1", "ws-a")).expect("src"), "win-m"),
        Some(("out-1".to_owned(), "ws-a".to_owned())),
        "complete A observation re-admits m"
    );
    // Retried send A->C (empty C) commits: the refused command was not lost
    // silently indefinitely.
    let retry = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr("send-3"),
        revision: 0,
        fingerprint: 7,
        domain: source_domain.clone(),
        domain_key: source_domain.key(),
        outer_gap: 0,
        focused_window: WindowId("win-m".to_owned()),
        windows: vec![
            carried("win-s1", "out-1", "ws-a", 0),
            carried("win-s2", "out-1", "ws-a", 10),
            carried("win-m", "out-1", "ws-a", 20),
        ],
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target_c.clone(), target_c.key())),
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::SendToWorkspace {
            window: "win-m".to_owned(),
            target_output: "out-1".to_owned(),
            target_workspace: "ws-c".to_owned(),
            follow: true,
        },
    };
    match engine.handle(&retry) {
        CoreReply::SendWorkspace(plan) => assert!(
            plan.geometry
                .iter()
                .any(|g| g.window.0 == "win-m" && g.output.0 == "out-1" && g.workspace.0 == "ws-c"),
            "retried send must carry the native assignment, got {:?}",
            plan.geometry
        ),
        other => panic!("retried send must commit after reconcile, got {other:?}"),
    }
    assert_eq!(
        homed(
            engine.session(&key("out-1", "ws-c")).expect("ws-c"),
            "win-m"
        ),
        Some(("out-1".to_owned(), "ws-c".to_owned()))
    );
}

// Causal state mirrors run-01dd52e26d954c9c act106/act109: the mover leaves
// ws-1 for ws-4, then returns with NO ws-1 reconcile in between, so
// last_active[ws-1] still names the departed mover leaf while the focus MRU
// holds the valid survivor. Ordinary open order (notepad, paint, terminal)
// via public Engine Reconcile builds the same causal focus history as the
// live spatial seed plus focus events: stack [notepad, paint], stale
// last_active terminal. Live physical inputs: 2560x1380 work area, gap 8,
// outer 8, minima notepad 401x246, paint 864x617, terminal 582x95. Carried
// rects are dummy full-bounds (topology derives from retained sessions;
// hints drive projection), unlike live native rects.
fn live_bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 2560,
        h: 1380,
    }
}
fn live_domain(workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: live_bounds(),
        gap: 8,
        adjacent: BTreeMap::new(),
    }
}
fn live_hints(window: &str) -> crate::size_hints::WindowSizeHints {
    let (min_w, min_h) = match window {
        "notepad" => (401, 246),
        "paint" => (864, 617),
        "terminal" => (582, 95),
        _ => return crate::size_hints::WindowSizeHints::none(),
    };
    crate::size_hints::WindowSizeHints {
        min_w: Some(min_w),
        min_h: Some(min_h),
        max_w: None,
        max_h: None,
    }
}
fn live_carried(window: &str, workspace: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: live_bounds(),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: live_hints(window),
    }
}
fn live_rec(
    domain_state: &OutputDomain,
    windows: Vec<EngineWindow>,
    focused: &str,
    c: &str,
) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr(c),
        revision: 0,
        fingerprint: 900,
        domain: domain_state.clone(),
        domain_key: domain_state.key(),
        outer_gap: 8,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Reconcile,
    }
}
fn live_send(
    source: &OutputDomain,
    target: &OutputDomain,
    src_rows: Vec<EngineWindow>,
    tgt_rows: Vec<EngineWindow>,
    window: &str,
    c: &str,
) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr(c),
        revision: 0,
        fingerprint: 901,
        domain: source.clone(),
        domain_key: source.key(),
        outer_gap: 8,
        focused_window: WindowId(window.to_owned()),
        windows: src_rows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target.clone(), target.key())),
        target_windows: tgt_rows,
        closed_windows: vec![],
        command: CoreCommand::SendToWorkspace {
            window: window.to_owned(),
            target_output: "mon-a".to_owned(),
            target_workspace: target.workspace.0.clone(),
            follow: true,
        },
    }
}
fn live_tree(engine: &Engine, workspace: &str) -> Option<Node> {
    engine
        .session(&key("mon-a", workspace))
        .expect("session")
        .snapshot()
        .domains
        .iter()
        .find(|d| d.workspace.0 == workspace)
        .expect("domain view")
        .tree
        .clone()
}

#[test]
fn send_return_after_send_away_splits_mru_tall_target() {
    use crate::directional::Axis;
    let ws1 = live_domain("ws-1");
    let ws4 = live_domain("ws-4");
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    // Ordinary open order builds focus [notepad, paint, terminal].
    for (rows, focused, c) in [
        (vec![live_carried("notepad", "ws-1")], "notepad", "live-r1"),
        (
            vec![
                live_carried("notepad", "ws-1"),
                live_carried("paint", "ws-1"),
            ],
            "paint",
            "live-r2",
        ),
        (
            vec![
                live_carried("notepad", "ws-1"),
                live_carried("paint", "ws-1"),
                live_carried("terminal", "ws-1"),
            ],
            "terminal",
            "live-r3",
        ),
    ] {
        match engine.handle(&live_rec(&ws1, rows, focused, c)) {
            CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
            other => panic!("{c} must project, got {other:?}"),
        }
    }
    // Mover away ws-1 -> ws-4 (empty target). last_active[ws-1] keeps the
    // departed terminal leaf; the ws-1 focus stack prunes to [notepad, paint].
    match engine.handle(&live_send(
        &ws1,
        &ws4,
        vec![
            live_carried("notepad", "ws-1"),
            live_carried("paint", "ws-1"),
            live_carried("terminal", "ws-1"),
        ],
        vec![],
        "terminal",
        "live-away",
    )) {
        CoreReply::SendWorkspace(_) => {}
        other => panic!("send-away must commit, got {other:?}"),
    }
    let src = engine.session(&key("mon-a", "ws-1")).expect("ws-1");
    assert_eq!(
        leaves_of(src, "mon-a", "ws-1"),
        vec!["leaf-notepad", "leaf-paint"],
        "send-away leaves two tall survivors on ws-1"
    );
    // Return with NO ws-1 reconcile in between: stale remembered terminal
    // must fall back to the MRU survivor (paint), splitting that tall leaf.
    let geometry = match engine.handle(&live_send(
        &ws4,
        &ws1,
        vec![live_carried("terminal", "ws-4")],
        vec![
            live_carried("notepad", "ws-1"),
            live_carried("paint", "ws-1"),
        ],
        "terminal",
        "live-back",
    )) {
        CoreReply::SendWorkspace(plan) => plan.geometry,
        other => panic!("send-back must commit SendWorkspace, got {other:?}"),
    };
    // Native assignment plus source removal.
    assert!(
        geometry
            .iter()
            .any(|g| g.window.0 == "terminal" && g.output.0 == "mon-a" && g.workspace.0 == "ws-1"),
        "return must carry the native assignment, got {geometry:?}"
    );
    assert!(
        engine.session(&key("mon-a", "ws-4")).is_none(),
        "ws-4 retires empty"
    );
    // Nested V topology splitting the MRU target: H[notepad V[paint terminal]].
    match live_tree(&engine, "ws-1") {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(axis, Axis::Horizontal, "ws-1 root stays width-split");
            assert_eq!(children.len(), 2);
            assert!(
                matches!(&children[0], Node::Leaf { id } if id.0 == "leaf-notepad"),
                "left leaf stays notepad, got {:?}",
                children[0]
            );
            match &children[1] {
                Node::Group {
                    axis,
                    children: inner,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Vertical, "tall paint target stacks arrival");
                    assert_eq!(inner.len(), 2);
                    assert!(
                        matches!(&inner[0], Node::Leaf { id } if id.0 == "leaf-paint"),
                        "split target is MRU paint, got {inner:?}"
                    );
                    assert!(
                        matches!(&inner[1], Node::Leaf { id } if id.0 == "leaf-terminal"),
                        "arrival follows paint, got {inner:?}"
                    );
                }
                other => panic!("expected nested V[paint terminal], got {other:?}"),
            }
        }
        other => panic!("expected H[notepad V[paint terminal]], got {other:?}"),
    }
    // Physical feasibility with actual minima: contained, minima met, and
    // exact column sharing (which already implies no overlap for this
    // topology, so no generic overlap loop).
    let b = live_bounds();
    let mut by_window = BTreeMap::new();
    for g in &geometry {
        if g.workspace.0 == "ws-1" {
            by_window.insert(g.window.0.clone(), g.rect);
        }
    }
    assert_eq!(
        by_window.len(),
        3,
        "ws-1 geometry covers all three, got {geometry:?}"
    );
    for (w, r) in &by_window {
        assert!(r.w > 0 && r.h > 0, "{w} positive, got {r:?}");
        assert!(
            r.x >= b.x && r.y >= b.y && r.x + r.w <= b.x + b.w && r.y + r.h <= b.y + b.h,
            "{w} contained in {b:?}, got {r:?}"
        );
        let (mw, mh) = match w.as_str() {
            "notepad" => (401, 246),
            "paint" => (864, 617),
            "terminal" => (582, 95),
            _ => (1, 1),
        };
        assert!(r.w >= mw && r.h >= mh, "{w} meets min {mw}x{mh}, got {r:?}");
    }
    let (n, p, t) = (
        by_window["notepad"],
        by_window["paint"],
        by_window["terminal"],
    );
    assert_eq!((p.x, p.w), (t.x, t.w), "paint/terminal share one column");
    assert_eq!(p.y + p.h + 8, t.y, "paint stacks above terminal with gap 8");
    assert_eq!(n.x + n.w + 8, p.x, "notepad sits left of the paint column");
    let committed_tree = live_tree(&engine, "ws-1");
    // Stable subsequent reconciliation preserves the repaired topology.
    match engine.handle(&live_rec(
        &ws1,
        vec![
            live_carried("notepad", "ws-1"),
            live_carried("paint", "ws-1"),
            live_carried("terminal", "ws-1"),
        ],
        "terminal",
        "live-rec",
    )) {
        CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
        other => panic!("ws-1 reconcile must project, got {other:?}"),
    }
    assert_eq!(live_tree(&engine, "ws-1"), committed_tree);
    let src = engine.session(&key("mon-a", "ws-1")).expect("ws-1");
    assert_eq!(
        leaves_of(src, "mon-a", "ws-1"),
        vec!["leaf-notepad", "leaf-paint", "leaf-terminal"]
    );
}

// ---- explicit output send (REQ-OUT-04, item 5.3/5.4) ----

fn seed_output_send_engine() -> (Engine, OutputDomain, OutputDomain, OutputDomain) {
    // Cross-output seeding: source out-1/ws-a (s1, mover s2), destination
    // out-2/ws-a (t1a, t1b, t2), third domain out-1/ws-b (b1).
    let mut source =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-a")]).expect("source");
    admit_to(&mut source, "s1", "out-1", "ws-a", "os-1");
    admit_to(&mut source, "s2", "out-1", "ws-a", "os-2");
    let mut target =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-2", "ws-a")]).expect("target");
    admit_to(&mut target, "t1a", "out-2", "ws-a", "ot-1");
    admit_to(&mut target, "t1b", "out-2", "ws-a", "ot-2");
    admit_to(&mut target, "t2", "out-2", "ws-a", "ot-3");
    let mut third =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-b")]).expect("third");
    admit_to(&mut third, "b1", "out-1", "ws-b", "ob-1");
    let source_domain = source.domains().first().expect("sd").clone();
    let target_domain = target.domains().first().expect("td").clone();
    let third_domain = third.domains().first().expect("thirdd").clone();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(key("out-1", "ws-a"), source, 0);
    engine.store_committed(key("out-2", "ws-a"), target, 0);
    engine.store_committed(key("out-1", "ws-b"), third, 0);
    (engine, source_domain, target_domain, third_domain)
}

#[allow(clippy::too_many_arguments)]
fn output_send_event(
    source: &OutputDomain,
    source_windows: Vec<EngineWindow>,
    mover: &str,
    target: &OutputDomain,
    target_windows: Vec<EngineWindow>,
    follow: bool,
    c: &str,
) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: corr(c),
        revision: 0,
        fingerprint: 7,
        domain: source.clone(),
        domain_key: source.key(),
        outer_gap: 0,
        focused_window: WindowId(mover.to_owned()),
        windows: source_windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target.clone(), target.key())),
        target_windows,
        closed_windows: vec![],
        command: CoreCommand::SendToOutput {
            window: mover.to_owned(),
            target_output: target.key().output.0.clone(),
            target_workspace: target.key().workspace.0.clone(),
            follow,
        },
    }
}

fn parent_of(tree: &Node, leaf: &str) -> Option<String> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            if children.iter().any(|c| c.id().0 == leaf) {
                return Some(id.0.clone());
            }
            children.iter().find_map(|c| parent_of(c, leaf))
        }
    }
}

fn tree_of_session(session: &Session, o: &str, ws: &str) -> Option<Node> {
    session
        .snapshot()
        .domains
        .into_iter()
        .find(|d| d.output.0 == o && d.workspace.0 == ws)
        .and_then(|d| d.tree)
}

#[test]
fn output_send_uses_focus_history_when_remembered_stale() {
    let (mut engine, source_domain, target_domain, third_domain) = seed_output_send_engine();
    // Stage the stale anchor: send t2 (the destination's remembered leaf) to
    // ws-b, leaving last-active pointing at a departed leaf while the
    // destination focus history retains t1b.
    let stage = output_send_event(
        &target_domain,
        vec![
            carried("t1a", "out-2", "ws-a", 0),
            carried("t1b", "out-2", "ws-a", 10),
            carried("t2", "out-2", "ws-a", 20),
        ],
        "t2",
        &third_domain,
        vec![carried("b1", "out-1", "ws-b", 0)],
        true,
        "out-eng-stage-1",
    );
    match engine.handle(&stage) {
        CoreReply::SendOutput(_) => {}
        other => panic!("staging send must commit SendOutput, got {other:?}"),
    }
    // Ordinary send of s2 onto out-2/ws-a: remembered t2 is stale, so the
    // destination focus history (t1b) anchors admission beside t1b.
    let event = output_send_event(
        &source_domain,
        vec![
            carried("s1", "out-1", "ws-a", 0),
            carried("s2", "out-1", "ws-a", 10),
        ],
        "s2",
        &target_domain,
        vec![
            carried("t1a", "out-2", "ws-a", 0),
            carried("t1b", "out-2", "ws-a", 10),
        ],
        true,
        "out-eng-mru-1",
    );
    let plan = match engine.handle(&event) {
        CoreReply::SendOutput(plan) => plan,
        other => panic!("output send must commit SendOutput, got {other:?}"),
    };
    assert!(plan.follow);
    assert_eq!(plan.focus_domain, Some(key("out-2", "ws-a")));
    assert_eq!(
        plan.focus_leaf.as_ref().map(|leaf| leaf.0.as_str()),
        Some("leaf-s2")
    );
    let tgt = engine.session(&key("out-2", "ws-a")).expect("tgt retained");
    let tree = tree_of_session(tgt, "out-2", "ws-a").expect("target tree");
    assert_eq!(leaves_of(tgt, "out-2", "ws-a").len(), 3);
    assert_eq!(
        parent_of(&tree, "leaf-s2"),
        parent_of(&tree, "leaf-t1b"),
        "mover splits beside the focus-history leaf"
    );
    assert_eq!(
        homed(tgt, "s2"),
        Some(("out-2".to_owned(), "ws-a".to_owned()))
    );
    // Both-domain geometry; the earlier send domain is undisturbed.
    let mut ids: Vec<(String, String)> = plan
        .geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.output.0.clone()))
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            ("s1".to_owned(), "out-1".to_owned()),
            ("s2".to_owned(), "out-2".to_owned()),
            ("t1a".to_owned(), "out-2".to_owned()),
            ("t1b".to_owned(), "out-2".to_owned()),
        ]
    );
    let third = engine
        .session(&key("out-1", "ws-b"))
        .expect("third retained");
    let mut ws_b = leaves_of(third, "out-1", "ws-b");
    ws_b.sort();
    assert_eq!(ws_b, vec!["leaf-b1".to_string(), "leaf-t2".to_string()]);
}

#[test]
fn output_send_empty_destination_admits_lone_root_follow_and_stay() {
    // Genuine root fallback through the Engine: the destination output was
    // never observed, so the mover becomes its lone root with ordinary
    // follow/stay focus. Stay with a sole source window leaves no focus and
    // retires the emptied source slot.
    let mut source =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-a")]).expect("source");
    admit_to(&mut source, "s1", "out-1", "ws-a", "oe-1");
    admit_to(&mut source, "m", "out-1", "ws-a", "oe-2");
    let source_domain = source.domains().first().expect("sd").clone();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(key("out-1", "ws-a"), source, 0);
    let fresh_target = domain("out-2", "ws-1");
    let follow = output_send_event(
        &source_domain,
        vec![
            carried("s1", "out-1", "ws-a", 0),
            carried("m", "out-1", "ws-a", 10),
        ],
        "m",
        &fresh_target,
        vec![],
        true,
        "out-eng-empty-follow-1",
    );
    let plan = match engine.handle(&follow) {
        CoreReply::SendOutput(plan) => plan,
        other => panic!("follow must commit SendOutput, got {other:?}"),
    };
    let tgt = engine
        .session(&key("out-2", "ws-1"))
        .expect("target created");
    assert_eq!(leaves_of(tgt, "out-2", "ws-1"), vec!["leaf-m".to_string()]);
    assert_eq!(plan.focus_domain, Some(key("out-2", "ws-1")));
    let mut ids: Vec<(String, String)> = plan
        .geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.output.0.clone()))
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            ("m".to_owned(), "out-2".to_owned()),
            ("s1".to_owned(), "out-1".to_owned()),
        ]
    );

    let mut solo_source =
        Session::new(owner(), generation(), 0, 7, vec![domain("out-1", "ws-a")]).expect("source");
    admit_to(&mut solo_source, "solo", "out-1", "ws-a", "oe-solo-1");
    let solo_domain = solo_source.domains().first().expect("sd").clone();
    let mut solo_engine = Engine::new();
    solo_engine.sync_binding(&owner(), &generation());
    solo_engine.store_committed(key("out-1", "ws-a"), solo_source, 0);
    let stay = output_send_event(
        &solo_domain,
        vec![carried("solo", "out-1", "ws-a", 0)],
        "solo",
        &domain("out-2", "ws-1"),
        vec![],
        false,
        "out-eng-empty-stay-1",
    );
    let stay_plan = match solo_engine.handle(&stay) {
        CoreReply::SendOutput(plan) => plan,
        other => panic!("stay must commit SendOutput, got {other:?}"),
    };
    assert!(!stay_plan.follow);
    assert_eq!(stay_plan.focus_domain, None);
    assert_eq!(stay_plan.focus_leaf, None);
    let stay_tgt = solo_engine
        .session(&key("out-2", "ws-1"))
        .expect("target created");
    assert_eq!(
        leaves_of(stay_tgt, "out-2", "ws-1"),
        vec!["leaf-solo".to_string()]
    );
    assert_eq!(stay_plan.geometry.len(), 1);
    assert_eq!(stay_plan.geometry[0].window.0, "solo");
    assert!(
        solo_engine.session(&key("out-1", "ws-a")).is_none(),
        "emptied source slot retires"
    );
}
