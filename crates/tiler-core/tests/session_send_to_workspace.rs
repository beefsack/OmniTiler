#![forbid(unsafe_code)]
//! Same-output send-to-workspace lifecycle tests (portable, headless).

use tiler_core::contract::{
    AckOutcome, AdapterAck, DivergenceKind, LIFECYCLE_POLICY_VERSION, LifecycleCapabilities,
    LifecycleCapability, LifecycleIntent, LifecycleOperation, LifecyclePostObservation,
    Observation,
};
use tiler_core::directional::{Axis, Node, NodeId, OutputId, Rule, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionBehavior, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError,
    RefusalKind, Session, SessionCommand, SessionObservation, SessionPlan,
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
fn full() -> LifecycleCapabilities {
    LifecycleCapabilities::full()
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
        adjacent: Default::default(),
    }
}
fn session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            dom("out-1", "ws-a"),
            dom("out-1", "ws-b"),
            dom("out-2", "ws-a"),
        ],
    )
    .expect("s")
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
fn obs(s: &Session, extra: Vec<ObservedWindow>) -> SessionObservation {
    let mut wins: Vec<ObservedWindow> = s
        .snapshot()
        .windows
        .iter()
        .map(|l| tiled(&l.window.0, &l.output.0, &l.workspace.0))
        .collect();
    wins.extend(s.exception_observed());
    wins.extend(extra);
    SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 100),
        windows: wins,
    }
}
fn ack_verify(s: &mut Session, plan: &SessionPlan, c: &str, fp: u64) {
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
        Observation::new(owner(), generation(), base, fp),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    s.verify_lifecycle(&post).expect("commit");
}
fn admit(s: &mut Session, w: &str, o: &str, ws: &str, pw: i32, ph: i32, c: &str) {
    let o0 = obs(s, vec![tiled(w, o, ws)]);
    let cmd = SessionCommand::Admit {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: Rect {
            x: 0,
            y: 0,
            w: pw,
            h: ph,
        },
        suppress_fixed_float: false,
    };
    let plan = s.propose(&cmd, &o0, &corr(c), &full()).expect("admit");
    ack_verify(s, &plan, c, 200);
}
fn mv(w: &str, o: &str, ws: &str) -> SessionCommand {
    SessionCommand::MoveToWorkspace {
        window: WindowId(w.to_owned()),
        target_output: OutputId(o.to_owned()),
        target_workspace: WorkspaceId(ws.to_owned()),
        follow: true,
    }
}
fn mv_stay(w: &str, o: &str, ws: &str) -> SessionCommand {
    SessionCommand::MoveToWorkspace {
        window: WindowId(w.to_owned()),
        target_output: OutputId(o.to_owned()),
        target_workspace: WorkspaceId(ws.to_owned()),
        follow: false,
    }
}
fn propose_mv(s: &mut Session, w: &str, o: &str, ws: &str, c: &str) -> SessionPlan {
    let o0 = obs(s, vec![]);
    s.propose(&mv(w, o, ws), &o0, &corr(c), &full())
        .expect("move proposes")
}
fn propose_stay(s: &mut Session, w: &str, o: &str, ws: &str, c: &str) -> SessionPlan {
    let o0 = obs(s, vec![]);
    s.propose(&mv_stay(w, o, ws), &o0, &corr(c), &full())
        .expect("stay proposes")
}
fn occupied_trio() -> Session {
    // Shared 3-source fixture with an occupied target: win-t1/win-t2 on
    // ws-b, win-1/win-2/win-3 on ws-a, focus ending on win-3.
    let mut s = session();
    admit(&mut s, "win-t1", "out-1", "ws-b", 120, 80, "occ-1");
    admit(&mut s, "win-t2", "out-1", "ws-b", 120, 80, "occ-2");
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "occ-3");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "occ-4");
    admit(&mut s, "win-3", "out-1", "ws-a", 120, 80, "occ-5");
    s
}
fn mr(s: &mut Session, cmd: &SessionCommand, c: &str, caps: &LifecycleCapabilities) -> RefusalKind {
    let o0 = obs(s, vec![]);
    match s.propose(cmd, &o0, &corr(c), caps) {
        Err(ProposeError::Refused(k)) => k,
        other => panic!("expected refusal, got {other:?}"),
    }
}
fn post_for(s: &Session, plan: &SessionPlan, c: &str, fp: u64) -> LifecyclePostObservation {
    let pre = plan.dispatch.preconditions.clone();
    let op = plan.dispatch.operation.clone();
    LifecyclePostObservation::new(
        Observation::new(owner(), generation(), s.accepted_revision(), fp),
        corr(c),
        true,
        pre,
        op,
    )
}
fn defer_float(s: &mut Session, w: &str, o: &str, ws: &str, c: &str) {
    let mut f = tiled(w, o, ws);
    f.floating = true;
    let o0 = obs(s, vec![f]);
    let mut flags = ExceptionFlags::none();
    flags.floating = true;
    let cmd = SessionCommand::Admit {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        exceptions: flags,
        exception_behavior: Some(ExceptionBehavior::Defer),
        placement_bounds: Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        suppress_fixed_float: false,
    };
    let dp = s.propose(&cmd, &o0, &corr(c), &full()).expect("defer");
    ack_verify(s, &dp, c, 210);
}
fn dk(o: &str, ws: &str) -> DomainKey {
    DomainKey {
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
    }
}
fn flat(t: Option<&Node>) -> Vec<String> {
    fn go(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Leaf { id } => out.push(id.0.clone()),
            Node::Group { children, .. } => children.iter().for_each(|c| go(c, out)),
        }
    }
    let mut v = vec![];
    if let Some(t) = t {
        go(t, &mut v);
    }
    v
}
fn at(ss: &[tiler_core::session::SessionDomainView], o: &str, ws: &str) -> Option<Node> {
    ss.iter()
        .find(|d| d.output.0 == o && d.workspace.0 == ws)
        .expect("d")
        .tree
        .clone()
}
fn s_leaves(s: &Session, o: &str, ws: &str) -> Vec<String> {
    flat(at(&s.snapshot().domains, o, ws).as_ref())
}
fn p_leaves(p: &SessionPlan, o: &str, ws: &str) -> Vec<String> {
    flat(at(&p.desired_snapshot.domains, o, ws).as_ref())
}

#[test]
fn send_follows_moved_window_into_target() {
    let mut s = session();
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "corr-1");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "corr-2");
    admit(&mut s, "win-3", "out-1", "ws-a", 80, 120, "corr-3");
    let plan = propose_mv(&mut s, "win-3", "out-1", "ws-b", "corr-4");
    assert!(matches!(
        (&plan.dispatch.intent, &plan.dispatch.operation),
        (LifecycleIntent::MoveToWorkspace { window, target_workspace, .. },
         LifecycleOperation::MoveTiled { leaf, source_workspace, target_workspace: tw, .. })
        if window.0 == "win-3" && target_workspace.0 == "ws-b" && leaf.0 == "leaf-win-3"
            && source_workspace.0 == "ws-a" && tw.0 == "ws-b"
    ));
    assert_eq!(
        plan.dispatch.required_capability,
        LifecycleCapability::MoveTiled
    );
    assert_eq!(plan.dispatch.policy_version, LIFECYCLE_POLICY_VERSION);
    assert_eq!(
        p_leaves(&plan, "out-1", "ws-a"),
        vec!["leaf-win-1", "leaf-win-2"]
    );
    assert_eq!(p_leaves(&plan, "out-1", "ws-b"), vec!["leaf-win-3"]);
    assert_eq!(plan.desired_focus_domain, Some(dk("out-1", "ws-b")));
    assert_eq!(
        plan.desired_focus_leaf,
        Some(NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(plan.desired_geometry.len(), 3);
    ack_verify(&mut s, &plan, "corr-4", 500);
    assert_eq!(s.snapshot(), plan.desired_snapshot);
    assert_eq!(
        s.focus(),
        (
            Some(dk("out-1", "ws-b")),
            Some(NodeId("leaf-win-3".to_owned()))
        )
    );
    // Sending the followed window back follows it again even though its
    // now-empty source has no remaining leaf.
    let lone = propose_mv(&mut s, "win-3", "out-1", "ws-a", "corr-5");
    assert!(at(&lone.desired_snapshot.domains, "out-1", "ws-b").is_none());
    assert_eq!(
        p_leaves(&lone, "out-1", "ws-a"),
        vec!["leaf-win-1", "leaf-win-2", "leaf-win-3"]
    );
    assert_eq!(lone.desired_focus_domain, Some(dk("out-1", "ws-a")));
    assert_eq!(
        lone.desired_focus_leaf,
        Some(NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(lone.desired_geometry.len(), 3);
    ack_verify(&mut s, &lone, "corr-5", 502);
    assert!(s_leaves(&s, "out-1", "ws-b").is_empty());
    assert_eq!(s.snapshot().domains.len(), 3);
}

#[test]
fn occupied_target_splits_remembered_leaf() {
    let mut s = session();
    admit(&mut s, "win-t1", "out-1", "ws-b", 120, 80, "corr-1");
    admit(&mut s, "win-t2", "out-1", "ws-b", 120, 80, "corr-2");
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "corr-3");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "corr-4");
    let plan = propose_mv(&mut s, "win-2", "out-1", "ws-b", "corr-5");
    assert_eq!(
        p_leaves(&plan, "out-1", "ws-b"),
        vec!["leaf-win-t1", "leaf-win-t2", "leaf-win-2"]
    );
    // Remembered t2 rect is narrow (60x80) so axis is Vertical, not the
    // output-bounds Horizontal; root keeps Horizontal with nested split.
    match at(&plan.desired_snapshot.domains, "out-1", "ws-b") {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(axis, Axis::Horizontal);
            assert_eq!(shares, vec![1, 1]);
            assert!(matches!(&children[0], Node::Leaf { id } if id.0 == "leaf-win-t1"));
            match &children[1] {
                Node::Group {
                    axis,
                    children: inner,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(*shares, vec![1, 1]);
                    assert!(matches!(&inner[0], Node::Leaf { id } if id.0 == "leaf-win-t2"));
                    assert!(matches!(&inner[1], Node::Leaf { id } if id.0 == "leaf-win-2"));
                }
                other => panic!("expected nested split, got {other:?}"),
            }
        }
        other => panic!("expected root group, got {other:?}"),
    }
    assert_eq!(p_leaves(&plan, "out-1", "ws-a"), vec!["leaf-win-1"]);
    assert_eq!(plan.desired_focus_domain, Some(dk("out-1", "ws-b")));
    assert_eq!(
        plan.desired_focus_leaf,
        Some(NodeId("leaf-win-2".to_owned()))
    );
    assert_eq!(plan.desired_geometry.len(), 4);
    ack_verify(&mut s, &plan, "corr-5", 501);
    assert_eq!(
        s_leaves(&s, "out-1", "ws-b"),
        vec!["leaf-win-t1", "leaf-win-t2", "leaf-win-2"]
    );
}

#[test]
fn refusals_fail_closed_then_lifecycle_diverges() {
    use tiler_core::reconcile::VerifyError;
    let mut s = session();
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "corr-1");
    admit(&mut s, "win-2", "out-1", "ws-b", 120, 80, "corr-2");
    let cases = [
        (
            "win-1",
            "out-1",
            "ws-b",
            "corr-3",
            RefusalKind::FocusMismatch,
        ),
        ("win-2", "out-1", "ws-b", "corr-4", RefusalKind::Unchanged),
        (
            "win-zzz",
            "out-1",
            "ws-a",
            "corr-5",
            RefusalKind::UnknownWindow,
        ),
        (
            "win-2",
            "out-1",
            "ws-zzz",
            "corr-6",
            RefusalKind::UnknownDomain,
        ),
        (
            "win-2",
            "out-2",
            "ws-a",
            "corr-7",
            RefusalKind::CrossDomainMismatch,
        ),
    ];
    for (w, o, ws, c, want) in cases {
        assert_eq!(mr(&mut s, &mv(w, o, ws), c, &full()), want);
    }
    let no_move = LifecycleCapabilities {
        admit_tiled: true,
        remove_tiled: true,
        move_tiled: false,
        toggle_orientation: true,
    };
    {
        let mut s_cap = session();
        admit(&mut s_cap, "win-1", "out-1", "ws-a", 120, 80, "corr-8a");
        admit(&mut s_cap, "win-2", "out-1", "ws-b", 120, 80, "corr-8b");
        let o_cap = obs(&s_cap, vec![]);
        let got = s_cap.propose(
            &mv("win-2", "out-1", "ws-a"),
            &o_cap,
            &corr("corr-8"),
            &no_move,
        );
        assert_eq!(
            got,
            Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
        );
        assert_eq!(s_cap.divergence(), Some(DivergenceKind::CapabilityRefused));
        assert!(!s_cap.has_pending());
        assert!(!s_cap.has_pending_desired());
    }
    let partial = SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 50),
        windows: vec![tiled("win-2", "out-1", "ws-b")],
    };
    let retry = mv("win-2", "out-1", "ws-a");
    let got = s.propose(&retry, &partial, &corr("corr-9"), &full());
    assert_eq!(
        got,
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert_eq!(
        mr(&mut s, &mv("win-2", "", "ws-a"), "corr-10", &full()),
        RefusalKind::MalformedInput
    );
    defer_float(&mut s, "win-9", "out-1", "ws-a", "corr-11");
    // D16 option (a): an ordinary intentional float rides the slotless
    // deferred transfer instead of refusing.
    let deferred = mv("win-9", "out-1", "ws-b");
    let deferred_plan = match s.propose(&deferred, &obs(&s, vec![]), &corr("corr-12"), &full()) {
        Ok(plan) => plan,
        other => panic!("ordinary float send must plan, got {other:?}"),
    };
    assert!(
        matches!(
            &deferred_plan.dispatch.operation,
            LifecycleOperation::MoveDeferred { .. }
        ),
        "float send carries the deferred transfer"
    );
    ack_verify(&mut s, &deferred_plan, "corr-12", 510);
    assert_eq!(s.divergence(), None);
    assert!(!s.has_pending());
    let plan = propose_mv(&mut s, "win-2", "out-1", "ws-a", "corr-13");
    assert!(s.has_pending());
    let o2 = obs(&s, vec![]);
    let dup = s.propose(
        &mv("win-2", "out-1", "ws-a"),
        &o2,
        &corr("corr-14"),
        &full(),
    );
    assert_eq!(dup, Err(ProposeError::PendingExists));
    // Verification before acknowledgement is rejected without divergence.
    let pre = post_for(&s, &plan, "corr-13", 511);
    assert_eq!(s.verify_lifecycle(&pre), Err(VerifyError::NotAcknowledged));
    let base = s.accepted_revision();
    let ack = AdapterAck::new(
        corr("corr-13"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    );
    s.acknowledge(&ack).expect("ack");
    let mut bad = plan.dispatch.operation.clone();
    if let LifecycleOperation::MoveTiled {
        ref mut target_workspace,
        ..
    } = bad
    {
        *target_workspace = WorkspaceId("ws-b".to_owned());
    } else {
        panic!("expected move-tiled");
    }
    let bad_post = LifecyclePostObservation::new(
        Observation::new(owner(), generation(), s.accepted_revision(), 512),
        corr("corr-13"),
        true,
        plan.dispatch.preconditions.clone(),
        bad,
    );
    assert_eq!(
        s.verify_lifecycle(&bad_post),
        Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
    );
    assert_eq!(s.divergence(), Some(DivergenceKind::PostconditionMismatch));
    assert!(!s.has_pending_desired());
    assert_eq!(s_leaves(&s, "out-1", "ws-b"), vec!["leaf-win-2"]);
}

#[test]
fn stay_keeps_source_selected_with_mru_focus() {
    let mut s = session();
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "corr-1");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "corr-2");
    admit(&mut s, "win-3", "out-1", "ws-a", 120, 80, "corr-3");
    let plan = propose_stay(&mut s, "win-3", "out-1", "ws-b", "corr-4");
    assert!(matches!(
        (&plan.dispatch.intent, &plan.dispatch.operation),
        (LifecycleIntent::MoveToWorkspace { window, target_workspace, follow, .. },
         LifecycleOperation::MoveTiled { leaf, source_workspace, target_workspace: tw, .. })
        if window.0 == "win-3" && target_workspace.0 == "ws-b" && !follow
            && leaf.0 == "leaf-win-3" && source_workspace.0 == "ws-a" && tw.0 == "ws-b"
    ));
    // Destination admission is unchanged: the mover lands as a lone root.
    assert_eq!(p_leaves(&plan, "out-1", "ws-b"), vec!["leaf-win-3"]);
    assert_eq!(
        p_leaves(&plan, "out-1", "ws-a"),
        vec!["leaf-win-1", "leaf-win-2"]
    );
    // Stay never selects the target: source MRU survivor keeps focus.
    assert_eq!(plan.desired_focus_domain, Some(dk("out-1", "ws-a")));
    assert_eq!(
        plan.desired_focus_leaf,
        Some(NodeId("leaf-win-2".to_owned()))
    );
    assert_eq!(plan.desired_geometry.len(), 3);
    ack_verify(&mut s, &plan, "corr-4", 500);
    assert_eq!(s.snapshot(), plan.desired_snapshot);
    assert_eq!(
        s.focus(),
        (
            Some(dk("out-1", "ws-a")),
            Some(NodeId("leaf-win-2".to_owned()))
        )
    );
}

#[test]
fn stay_of_sole_source_window_leaves_no_focus() {
    let mut s = session();
    admit(&mut s, "win-solo", "out-1", "ws-a", 120, 80, "corr-1");
    let plan = propose_stay(&mut s, "win-solo", "out-1", "ws-b", "corr-2");
    assert_eq!(p_leaves(&plan, "out-1", "ws-b"), vec!["leaf-win-solo"]);
    assert!(at(&plan.desired_snapshot.domains, "out-1", "ws-a").is_none());
    assert_eq!(plan.desired_focus_domain, None);
    assert_eq!(plan.desired_focus_leaf, None);
    assert_eq!(plan.desired_geometry.len(), 1);
    ack_verify(&mut s, &plan, "corr-2", 501);
    assert!(s_leaves(&s, "out-1", "ws-a").is_empty());
    assert_eq!(s.focus(), (None, None));
}

#[test]
fn follow_and_stay_share_transfer_with_focus_only_difference() {
    // Same 3-source fixture with an occupied target: follow and stay propose
    // byte-identical transfer state (snapshot, geometry, admission
    // operation) and differ only in the intent follow flag plus desired
    // focus. Stay focus equals an ordinary focused removal on the same
    // state, proving stay reuses the existing removal MRU path.
    let mut s_follow = occupied_trio();
    let mut s_stay = occupied_trio();
    let mut s_remove = occupied_trio();
    let follow = propose_mv(&mut s_follow, "win-3", "out-1", "ws-b", "corr-x1");
    let stay = propose_stay(&mut s_stay, "win-3", "out-1", "ws-b", "corr-x2");
    let o_remove = obs(&s_remove, vec![]);
    let removed = s_remove
        .propose(
            &SessionCommand::Remove {
                window: WindowId("win-3".to_owned()),
            },
            &o_remove,
            &corr("corr-x3"),
            &full(),
        )
        .expect("remove proposes");
    assert_eq!(follow.desired_snapshot, stay.desired_snapshot);
    assert_eq!(follow.desired_geometry, stay.desired_geometry);
    assert_eq!(follow.dispatch.operation, stay.dispatch.operation);
    assert_eq!(
        follow.dispatch.intent,
        LifecycleIntent::MoveToWorkspace {
            window: WindowId("win-3".to_owned()),
            target_output: OutputId("out-1".to_owned()),
            target_workspace: WorkspaceId("ws-b".to_owned()),
            follow: true,
        }
    );
    assert_eq!(
        stay.dispatch.intent,
        LifecycleIntent::MoveToWorkspace {
            window: WindowId("win-3".to_owned()),
            target_output: OutputId("out-1".to_owned()),
            target_workspace: WorkspaceId("ws-b".to_owned()),
            follow: false,
        }
    );
    assert_eq!(follow.desired_focus_domain, Some(dk("out-1", "ws-b")));
    assert_eq!(
        follow.desired_focus_leaf,
        Some(NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(stay.desired_focus_domain, removed.desired_focus_domain);
    assert_eq!(stay.desired_focus_leaf, removed.desired_focus_leaf);
    assert_eq!(stay.desired_focus_domain, Some(dk("out-1", "ws-a")));
    assert_eq!(
        stay.desired_focus_leaf,
        Some(NodeId("leaf-win-2".to_owned()))
    );
    ack_verify(&mut s_follow, &follow, "corr-x1", 600);
    ack_verify(&mut s_stay, &stay, "corr-x2", 600);
    assert_eq!(
        s_follow.focus(),
        (
            Some(dk("out-1", "ws-b")),
            Some(NodeId("leaf-win-3".to_owned()))
        )
    );
    assert_eq!(
        s_stay.focus(),
        (
            Some(dk("out-1", "ws-a")),
            Some(NodeId("leaf-win-2".to_owned()))
        )
    );
}

#[test]
fn post_stay_observation_keeps_native_focus_over_mru() {
    // After a stay send commits with source MRU focus (win-2), a post-hoc
    // observation naming a different native focus (win-1) stays
    // authoritative: converge keeps the observed focus instead of snapping
    // back to the MRU. Reconcile policy is unchanged by the stay addition;
    // this pins the existing external-observation behavior on the stay flow.
    let mut s = session();
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "corr-1");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "corr-2");
    admit(&mut s, "win-3", "out-1", "ws-a", 120, 80, "corr-3");
    let stay = propose_stay(&mut s, "win-3", "out-1", "ws-b", "corr-4");
    ack_verify(&mut s, &stay, "corr-4", 500);
    assert_eq!(
        s.focus(),
        (
            Some(dk("out-1", "ws-a")),
            Some(NodeId("leaf-win-2".to_owned()))
        )
    );
    let observed = obs(&s, vec![]);
    let counts = s
        .converge_observation(&observed, Some(&WindowId("win-1".to_owned())))
        .expect("converges");
    assert_eq!(counts.removed, 0);
    assert_eq!(counts.admitted, 0);
    assert_eq!(counts.flags_adopted, 0);
    assert_eq!(
        s.focus(),
        (
            Some(dk("out-1", "ws-a")),
            Some(NodeId("leaf-win-1".to_owned()))
        )
    );
}

#[test]
fn r1_r4_untouched() {
    const RULES: [Rule; 6] = [
        Rule::R1,
        Rule::R2a,
        Rule::R2b,
        Rule::R2c,
        Rule::R3,
        Rule::R4,
    ];
    assert_eq!(RULES.len(), 6);
    assert_ne!(
        LifecycleCapability::MoveTiled,
        LifecycleCapability::AdmitTiled
    );
    assert!(full().supports(LifecycleCapability::MoveTiled));
}

// ---- explicit output send (REQ-OUT-04, item 5.3/5.4) ----

fn out_mv(w: &str, o: &str, ws: &str, follow: bool) -> SessionCommand {
    SessionCommand::MoveToOutput {
        window: WindowId(w.to_owned()),
        target_output: OutputId(o.to_owned()),
        target_workspace: WorkspaceId(ws.to_owned()),
        follow,
    }
}
fn propose_out(s: &mut Session, w: &str, o: &str, ws: &str, follow: bool, c: &str) -> SessionPlan {
    // Mirror the Engine path: sync session focus from the observed focused
    // (mover) window before proposing, since admits leave focus on the last
    // admitted domain which may be the target.
    let link = s
        .snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == w)
        .unwrap_or_else(|| panic!("mover link {w}"))
        .clone();
    assert!(s.sync_focus_from_window(&dk(&link.output.0, &link.workspace.0), &link.window));
    let o0 = obs(s, vec![]);
    s.propose(&out_mv(w, o, ws, follow), &o0, &corr(c), &full())
        .unwrap_or_else(|e| panic!("output send proposes: {e:?}"))
}
fn parent_of(tree: &Node, leaf: &str) -> Option<NodeId> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            if children.iter().any(|c| c.id().0 == leaf) {
                return Some(id.clone());
            }
            children.iter().find_map(|c| parent_of(c, leaf))
        }
    }
}

#[test]
fn output_send_follow_moves_to_destination_current_workspace() {
    // Follow carries the mover to the destination output's current workspace
    // (resolved adapter-side, here out-2/ws-a): source collapses, focus and
    // the retargeted link follow, geometry covers both domains.
    let mut s = session();
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "out-1");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "out-2");
    admit(&mut s, "win-t", "out-2", "ws-a", 120, 80, "out-3");
    let plan = propose_out(&mut s, "win-2", "out-2", "ws-a", true, "out-4");
    assert!(matches!(
        (&plan.dispatch.intent, &plan.dispatch.operation),
        (
            LifecycleIntent::MoveToOutput { window, target_output, target_workspace, follow: true },
            LifecycleOperation::MoveTiled { leaf, source_output, target_output: to, .. }
        )
        if window.0 == "win-2" && target_output.0 == "out-2" && target_workspace.0 == "ws-a"
            && leaf.0 == "leaf-win-2" && source_output.0 == "out-1" && to.0 == "out-2"
    ));
    assert_eq!(plan.desired_focus_domain, Some(dk("out-2", "ws-a")));
    assert_eq!(
        plan.desired_focus_leaf,
        Some(NodeId("leaf-win-2".to_owned()))
    );
    ack_verify(&mut s, &plan, "out-4", 300);
    assert_eq!(
        s_leaves(&s, "out-1", "ws-a"),
        vec!["leaf-win-1".to_string()]
    );
    assert!(s_leaves(&s, "out-2", "ws-a").contains(&"leaf-win-2".to_string()));
    let link = s
        .snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == "win-2")
        .expect("mover link")
        .clone();
    assert_eq!(link.output.0, "out-2");
    assert_eq!(link.workspace.0, "ws-a");
    assert_eq!(
        s.focus(),
        (
            Some(dk("out-2", "ws-a")),
            Some(NodeId("leaf-win-2".to_owned()))
        )
    );
}

#[test]
fn output_send_stay_keeps_source_mru_without_selecting_target() {
    // Stay moves the window but leaves the source selected: the source MRU
    // survivor keeps focus and the operation echoes follow=false.
    // Destination admission matches the follow path.
    fn trio(tag: &str) -> Session {
        let mut s = session();
        admit(
            &mut s,
            "win-1",
            "out-1",
            "ws-a",
            120,
            80,
            &format!("{tag}-1"),
        );
        admit(
            &mut s,
            "win-2",
            "out-1",
            "ws-a",
            120,
            80,
            &format!("{tag}-2"),
        );
        admit(
            &mut s,
            "win-3",
            "out-1",
            "ws-a",
            120,
            80,
            &format!("{tag}-3"),
        );
        admit(
            &mut s,
            "win-t",
            "out-2",
            "ws-a",
            120,
            80,
            &format!("{tag}-4"),
        );
        s
    }
    let mut s_follow = trio("stay-f");
    let follow_plan = propose_out(&mut s_follow, "win-3", "out-2", "ws-a", true, "stay-f5");
    let mut s_stay = trio("stay-s");
    let stay_plan = propose_out(&mut s_stay, "win-3", "out-2", "ws-a", false, "stay-s5");
    // Identical destination topology/geometry/operation; only focus differs.
    assert_eq!(
        p_leaves(&stay_plan, "out-2", "ws-a"),
        p_leaves(&follow_plan, "out-2", "ws-a")
    );
    assert_eq!(
        stay_plan.desired_geometry, follow_plan.desired_geometry,
        "stay/follow share destination geometry"
    );
    assert!(matches!(
        &stay_plan.dispatch.intent,
        LifecycleIntent::MoveToOutput { follow: false, .. }
    ));
    assert_eq!(stay_plan.desired_focus_domain, Some(dk("out-1", "ws-a")));
    assert_eq!(
        stay_plan.desired_focus_leaf,
        Some(NodeId("leaf-win-2".to_owned())),
        "source MRU survivor keeps focus"
    );
    ack_verify(&mut s_stay, &stay_plan, "stay-s5", 310);
    assert_eq!(
        s_stay.focus(),
        (
            Some(dk("out-1", "ws-a")),
            Some(NodeId("leaf-win-2".to_owned()))
        )
    );
    assert!(s_leaves(&s_stay, "out-2", "ws-a").contains(&"leaf-win-3".to_string()));
}

#[test]
fn output_send_admits_at_remembered_leaf() {
    // Ordinary admission: the mover splits beside the destination's
    // remembered last-active leaf (here win-t2, focused last on the target).
    let mut s = session();
    admit(&mut s, "win-t1", "out-2", "ws-a", 120, 80, "rem-1");
    admit(&mut s, "win-t2", "out-2", "ws-a", 120, 80, "rem-2");
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "rem-3");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "rem-4");
    let plan = propose_out(&mut s, "win-2", "out-2", "ws-a", true, "rem-5");
    let target = at(&plan.desired_snapshot.domains, "out-2", "ws-a").expect("target");
    assert_eq!(flat(Some(&target)).len(), 3);
    let mover_parent = parent_of(&target, "leaf-win-2").expect("mover parent");
    assert_eq!(
        parent_of(&target, "leaf-win-t2"),
        Some(mover_parent),
        "mover splits beside the remembered leaf"
    );
    ack_verify(&mut s, &plan, "rem-5", 320);
    assert!(s_leaves(&s, "out-2", "ws-a").contains(&"leaf-win-2".to_string()));
}

#[test]
fn output_send_scope_refusals_keep_ops_distinct() {
    // Explicit output send is DISTINCT from workspace send: same-output
    // targets refuse (that scope belongs to `MoveToWorkspace`), unknown
    // targets refuse as UnknownDomain. Same-domain is same-output, hence
    // CrossDomainMismatch rather than Unchanged.
    let mut s = session();
    admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, "scope-1");
    admit(&mut s, "win-2", "out-1", "ws-a", 120, 80, "scope-2");
    assert_eq!(
        mr(
            &mut s,
            &out_mv("win-2", "out-1", "ws-b", true),
            "scope-3",
            &full()
        ),
        RefusalKind::CrossDomainMismatch,
        "same-output distinct workspace belongs to workspace send"
    );
    assert_eq!(
        mr(
            &mut s,
            &out_mv("win-2", "out-1", "ws-a", true),
            "scope-4",
            &full()
        ),
        RefusalKind::CrossDomainMismatch,
        "same-domain is same-output for output send"
    );
    assert_eq!(
        mr(
            &mut s,
            &out_mv("win-2", "out-9", "ws-z", true),
            "scope-5",
            &full()
        ),
        RefusalKind::UnknownDomain
    );
    // Workspace send keeps refusing cross-output targets (unchanged).
    assert_eq!(
        mr(
            &mut s,
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-2".to_owned()),
                target_output: OutputId("out-2".to_owned()),
                target_workspace: WorkspaceId("ws-a".to_owned()),
                follow: true,
            },
            "scope-6",
            &full()
        ),
        RefusalKind::CrossDomainMismatch
    );
}

#[test]
fn output_send_refuses_sticky_but_carries_intentional_floats() {
    // D16 option (a), no setting: ordinary intentional floats ride
    // the slotless deferred transfer like workspace send (float
    // retained on arrival, no tree mutation); sticky exception movers
    // keep refusing as NotTiled.
    for (flag, admit_c, mr_c) in [
        ("floating", "elig-f1", "elig-f2"),
        ("sticky", "elig-s1", "elig-s2"),
    ] {
        let mut s = session();
        admit(&mut s, "win-1", "out-1", "ws-a", 120, 80, admit_c);
        let mut row = tiled("win-f", "out-1", "ws-a");
        if flag == "floating" {
            row.floating = true;
        } else {
            row.sticky = true;
        }
        let o0 = obs(&s, vec![row]);
        let mut flags = ExceptionFlags::none();
        if flag == "floating" {
            flags.floating = true;
        } else {
            flags.sticky = true;
        }
        let cmd = SessionCommand::Admit {
            window: WindowId("win-f".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-a".to_owned()),
            exceptions: flags,
            exception_behavior: Some(ExceptionBehavior::Defer),
            placement_bounds: Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            suppress_fixed_float: false,
        };
        let dp = s
            .propose(&cmd, &o0, &corr(admit_c), &full())
            .expect("defer");
        ack_verify(&mut s, &dp, admit_c, 330);
        if flag == "sticky" {
            assert_eq!(
                mr(
                    &mut s,
                    &out_mv("win-f", "out-2", "ws-a", true),
                    mr_c,
                    &full()
                ),
                RefusalKind::NotTiled,
                "sticky movers stay ineligible"
            );
            continue;
        }
        let plan = s
            .propose(
                &out_mv("win-f", "out-2", "ws-a", true),
                &obs(&s, vec![]),
                &corr(mr_c),
                &full(),
            )
            .expect("ordinary float output send plans");
        assert!(
            matches!(
                &plan.dispatch.operation,
                LifecycleOperation::MoveDeferred { .. }
            ),
            "ordinary floats ride the deferred transfer"
        );
        ack_verify(&mut s, &plan, mr_c, 340);
        let record = s
            .exception_observed()
            .into_iter()
            .find(|w| w.window.0 == "win-f")
            .expect("mover exception");
        assert_eq!(record.output.0, "out-2");
        assert_eq!(record.workspace.0, "ws-a");
        assert!(record.floating, "float retained across outputs");
    }
}

#[test]
fn output_send_uses_focus_history_when_remembered_stale() {
    // 5.4 destination focus-history fallback: the destination's remembered
    // last-active leaf (win-a3) departed via an earlier output send, leaving
    // a stale anchor, while the destination focus history retains win-a2.
    // The mover must split beside the history leaf (not the stale remembered
    // leaf, not a root wrap): same parent as win-a2 in the desired target.
    let mut s = session();
    admit(&mut s, "win-a1", "out-2", "ws-a", 120, 80, "mru-1");
    admit(&mut s, "win-a2", "out-2", "ws-a", 120, 80, "mru-2");
    admit(&mut s, "win-a3", "out-2", "ws-a", 120, 80, "mru-3");
    admit(&mut s, "win-b1", "out-1", "ws-b", 120, 80, "mru-4");
    let first = propose_out(&mut s, "win-a3", "out-1", "ws-b", true, "mru-5");
    ack_verify(&mut s, &first, "mru-5", 400);
    assert_eq!(s_leaves(&s, "out-2", "ws-a").len(), 2);
    admit(&mut s, "win-c1", "out-1", "ws-a", 120, 80, "mru-6");
    admit(&mut s, "win-c2", "out-1", "ws-a", 120, 80, "mru-7");
    let plan = propose_out(&mut s, "win-c2", "out-2", "ws-a", true, "mru-8");
    assert!(matches!(
        &plan.dispatch.intent,
        LifecycleIntent::MoveToOutput { follow: true, .. }
    ));
    let target = at(&plan.desired_snapshot.domains, "out-2", "ws-a").expect("target");
    assert_eq!(flat(Some(&target)).len(), 3);
    let mover_parent = parent_of(&target, "leaf-win-c2").expect("mover parent");
    assert_eq!(
        parent_of(&target, "leaf-win-a2"),
        Some(mover_parent),
        "mover splits beside the focus-history leaf, not a root wrap"
    );
    assert_eq!(plan.desired_focus_domain, Some(dk("out-2", "ws-a")));
    assert_eq!(
        plan.desired_focus_leaf,
        Some(NodeId("leaf-win-c2".to_owned()))
    );
    ack_verify(&mut s, &plan, "mru-8", 410);
    // Source collapses; the earlier send domain is undisturbed.
    assert_eq!(
        s_leaves(&s, "out-1", "ws-a"),
        vec!["leaf-win-c1".to_string()]
    );
    let mut ws_b = s_leaves(&s, "out-1", "ws-b");
    ws_b.sort();
    assert_eq!(
        ws_b,
        vec!["leaf-win-a3".to_string(), "leaf-win-b1".to_string()]
    );
    assert!(s_leaves(&s, "out-2", "ws-a").contains(&"leaf-win-c2".to_string()));
}

#[test]
fn output_send_empty_destination_admits_lone_root_follow_and_stay() {
    // Genuine root fallback: no remembered leaf and no destination focus
    // history exist (destination never admitted), so the mover becomes the
    // lone root. Follow focuses the mover on the target; stay with a sole
    // source window leaves no focus, and no other domain is disturbed.
    fn pair(tag: &str) -> Session {
        let mut s = session();
        admit(
            &mut s,
            "win-1",
            "out-1",
            "ws-a",
            120,
            80,
            &format!("{tag}-1"),
        );
        admit(
            &mut s,
            "win-2",
            "out-1",
            "ws-a",
            120,
            80,
            &format!("{tag}-2"),
        );
        s
    }
    let mut s_follow = pair("empty-f");
    let follow_plan = propose_out(&mut s_follow, "win-2", "out-2", "ws-a", true, "empty-f3");
    assert_eq!(
        p_leaves(&follow_plan, "out-2", "ws-a"),
        vec!["leaf-win-2".to_string()],
        "mover is the lone root of the empty destination"
    );
    assert_eq!(follow_plan.desired_focus_domain, Some(dk("out-2", "ws-a")));
    assert_eq!(
        follow_plan.desired_focus_leaf,
        Some(NodeId("leaf-win-2".to_owned()))
    );
    ack_verify(&mut s_follow, &follow_plan, "empty-f3", 420);
    assert_eq!(
        s_leaves(&s_follow, "out-1", "ws-a"),
        vec!["leaf-win-1".to_string()]
    );
    assert!(s_leaves(&s_follow, "out-1", "ws-b").is_empty());

    let mut s_stay = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            dom("out-1", "ws-a"),
            dom("out-1", "ws-b"),
            dom("out-2", "ws-a"),
        ],
    )
    .expect("s");
    admit(&mut s_stay, "solo", "out-1", "ws-a", 120, 80, "empty-s1");
    let stay_plan = propose_out(&mut s_stay, "solo", "out-2", "ws-a", false, "empty-s2");
    assert_eq!(
        p_leaves(&stay_plan, "out-2", "ws-a"),
        vec!["leaf-solo".to_string()]
    );
    assert_eq!(stay_plan.desired_focus_domain, None);
    assert_eq!(stay_plan.desired_focus_leaf, None);
    ack_verify(&mut s_stay, &stay_plan, "empty-s2", 430);
    assert!(s_leaves(&s_stay, "out-1", "ws-a").is_empty());
    assert_eq!(s_stay.focus(), (None, None));
}
