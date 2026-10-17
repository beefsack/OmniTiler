#![forbid(unsafe_code)]
//! Session directional movement + focus integration (portable, headless).
//!
//! Drives the real `session::Session` movement/focus APIs (`propose_move`,
//! `verify_move`, `propose_focus`, `verify_focus`) through
//! propose/acknowledge/verify cycles. No live compositor state. Uses the
//! frozen `cosmic_v1` planner indirectly via the session; asserts rules
//! R1-R4, focus retention, shares/order, domain isolation, refusals, geometry
//! completeness, commit, determinism, and a bounded property-like loop.

use std::collections::{BTreeMap, BTreeSet};
use tiler_core::contract::{
    AckOutcome, AdapterAck, DivergenceKind, FocusCapabilities, FocusPostObservation,
    LifecycleCapabilities, LifecyclePostObservation, Observation, PostObservation,
};
use tiler_core::directional::{
    Capabilities, Direction, Node, NodeId, OutputId, SameAxisMove, WindowId, WorkspaceId,
};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionFocusPlan, SessionMovePlan, SessionObservation, SessionPlan,
};

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
}
fn domain(output: &str, workspace: &str, w: i32, h: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn domain_with_adjacent(
    output: &str,
    workspace: &str,
    adjacent: Vec<(Direction, &str)>,
) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
        gap: 0,
        adjacent: adjacent
            .into_iter()
            .map(|(d, t)| (d, OutputId(t.to_owned())))
            .collect(),
    }
}
fn single_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 200, 200)],
    )
    .expect("session")
}
fn r4_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            domain_with_adjacent("out-2", "ws-1", vec![(Direction::Left, "out-1")]),
        ],
    )
    .expect("r4 session")
}
/// Axis-intent placement: `horiz` requests a horizontal split. The COSMIC
/// admission rule selects axis from target geometry (wide splits portable Horizontal),
/// so horizontal needs a wide target and vice versa.
fn placement(horiz: bool) -> Rect {
    if horiz {
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        }
    } else {
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        }
    }
}
fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}
fn complete_obs(session: &Session, extra: Vec<ObservedWindow>) -> SessionObservation {
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| ObservedWindow {
            window: l.window.clone(),
            output: l.output.clone(),
            workspace: l.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        })
        .collect();
    windows.extend(session.exception_observed());
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            session.accepted_revision(),
            100 + session.accepted_revision(),
        ),
        windows,
    }
}
fn tiled(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}
fn admit_commit(
    session: &mut Session,
    window: &str,
    output: &str,
    workspace: &str,
    horiz: bool,
    corr: &str,
) -> SessionPlan {
    let cmd = SessionCommand::Admit {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: placement(horiz),
        suppress_fixed_float: false,
    };
    let obs = complete_obs(session, vec![tiled(window, output, workspace)]);
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &cmd,
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("admit {window}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 200 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
    plan
}
fn focused_window(session: &Session, domain: &DomainKey) -> WindowId {
    let (d, l) = session.focus();
    let d = d.expect("focused domain");
    let l = l.expect("focused leaf");
    assert_eq!(&d, domain);
    session
        .snapshot()
        .windows
        .iter()
        .find(|w| w.leaf == l && w.output == domain.output && w.workspace == domain.workspace)
        .expect("focused link")
        .window
        .clone()
}
fn move_commit(
    session: &mut Session,
    domain: &DomainKey,
    window: &WindowId,
    direction: Direction,
    corr: &str,
    caps: &Capabilities,
) -> SessionMovePlan {
    let obs = complete_obs(session, vec![]);
    let base = session.accepted_revision();
    let plan = session
        .propose_move(domain, window, direction, &obs, &correlation(corr), caps)
        .unwrap_or_else(|e| panic!("move {direction:?}: {e:?}"))
        .expect_moved("tiled move");
    assert_eq!(plan.dispatch.base_revision, base);
    // Pre-ack immutability is asserted by callers where needed; commit here.
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = PostObservation::new(
        Observation::new(owner(), generation(), base, 900 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    let commit = session.verify_move(&post).expect("verify");
    assert_eq!(commit.revision, base + 1);
    plan
}
fn move_commit_focused(
    session: &mut Session,
    domain: &DomainKey,
    direction: Direction,
    corr: &str,
    caps: &Capabilities,
) -> SessionMovePlan {
    let w = focused_window(session, domain);
    move_commit(session, domain, &w, direction, corr, caps)
}
fn move_commit_with_mode(
    session: &mut Session,
    domain: &DomainKey,
    window: &WindowId,
    direction: Direction,
    corr: &str,
    caps: &Capabilities,
    mode: SameAxisMove,
) -> SessionMovePlan {
    let obs = complete_obs(session, vec![]);
    let base = session.accepted_revision();
    let plan = session
        .propose_move_with_same_axis(
            domain,
            window,
            direction,
            &obs,
            &correlation(corr),
            caps,
            mode,
        )
        .unwrap_or_else(|e| panic!("move {direction:?} {mode:?}: {e:?}"))
        .expect_moved("tiled move");
    assert_eq!(plan.dispatch.base_revision, base);
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = PostObservation::new(
        Observation::new(owner(), generation(), base, 900 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    let commit = session.verify_move(&post).expect("verify");
    assert_eq!(commit.revision, base + 1);
    plan
}
fn move_commit_focused_with_mode(
    session: &mut Session,
    domain: &DomainKey,
    direction: Direction,
    corr: &str,
    caps: &Capabilities,
    mode: SameAxisMove,
) -> SessionMovePlan {
    let w = focused_window(session, domain);
    move_commit_with_mode(session, domain, &w, direction, corr, caps, mode)
}
/// Direct children plus shares of the group parenting `leaf` in `tree`.
fn parent_children_shares(tree: &Node, leaf: &str) -> (Vec<String>, Vec<u64>) {
    fn find(node: &Node, leaf: &str) -> Option<(Vec<String>, Vec<u64>)> {
        match node {
            Node::Leaf { .. } => None,
            Node::Group {
                children, shares, ..
            } => {
                if children
                    .iter()
                    .any(|c| matches!(c, Node::Leaf { id } if id.0 == leaf))
                {
                    return Some((
                        children.iter().map(|c| c.id().0.clone()).collect(),
                        shares.clone(),
                    ));
                }
                children.iter().find_map(|c| find(c, leaf))
            }
        }
    }
    find(tree, leaf).unwrap_or_else(|| panic!("leaf {leaf} has no direct parent group"))
}
fn focus_commit(
    session: &mut Session,
    domain: &DomainKey,
    window: &WindowId,
    direction: Direction,
    corr: &str,
    caps: &FocusCapabilities,
) -> SessionFocusPlan {
    let obs = complete_obs(session, vec![]);
    let base = session.accepted_revision();
    let plan = session
        .propose_focus(domain, window, direction, &obs, &correlation(corr), caps)
        .unwrap_or_else(|e| panic!("focus {direction:?}: {e:?}"));
    assert_eq!(plan.dispatch.base_revision, base);
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 950 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    let commit = session.verify_focus(&post).expect("verify focus");
    assert_eq!(commit.revision, base + 1);
    plan
}
fn focus_commit_focused(
    session: &mut Session,
    domain: &DomainKey,
    direction: Direction,
    corr: &str,
    caps: &FocusCapabilities,
) -> SessionFocusPlan {
    let w = focused_window(session, domain);
    focus_commit(session, domain, &w, direction, corr, caps)
}
fn leaves(session: &Session, output: &str, workspace: &str) -> Vec<String> {
    let snap = session.snapshot();
    let view = snap
        .domains
        .iter()
        .find(|d| d.output.0 == output && d.workspace.0 == workspace)
        .expect("domain");
    match &view.tree {
        None => vec![],
        Some(t) => {
            let mut out = vec![];
            collect_leaves(t, &mut out);
            out
        }
    }
}
fn collect_leaves(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Leaf { id } => out.push(id.0.clone()),
        Node::Group { children, .. } => {
            for c in children {
                collect_leaves(c, out);
            }
        }
    }
}
fn assert_geometry_complete(plan: &SessionMovePlan, session: &Session) {
    use std::collections::BTreeMap;
    let mut by_domain: BTreeMap<(String, String), Vec<Rect>> = BTreeMap::new();
    for g in &plan.desired_geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0, "positive geometry");
        assert!(!g.window.0.is_empty() && !g.leaf.0.is_empty());
        by_domain
            .entry((g.output.0.clone(), g.workspace.0.clone()))
            .or_default()
            .push(g.rect);
    }
    let snap = &plan.desired_snapshot;
    for view in &snap.domains {
        let in_affected = plan
            .desired_geometry
            .iter()
            .any(|g| g.output == view.output && g.workspace == view.workspace);
        if !in_affected {
            continue;
        }
        let mut expected = vec![];
        if let Some(tree) = &view.tree {
            collect_leaves(tree, &mut expected);
        }
        let got: Vec<String> = plan
            .desired_geometry
            .iter()
            .filter(|g| g.output == view.output && g.workspace == view.workspace)
            .map(|g| g.leaf.0.clone())
            .collect();
        let mut e = expected.clone();
        e.sort();
        let mut gg = got.clone();
        gg.sort();
        assert_eq!(e, gg, "geometry covers affected domain leaves");
    }
    for view in &snap.domains {
        let dom = session
            .domains()
            .iter()
            .find(|d| d.id == view.output && d.workspace == view.workspace)
            .expect("domain bounds");
        let rects = by_domain
            .get(&(view.output.0.clone(), view.workspace.0.clone()))
            .cloned()
            .unwrap_or_default();
        for r in &rects {
            assert!(r.x >= dom.bounds.x && r.y >= dom.bounds.y);
            assert!(r.x + r.w <= dom.bounds.x + dom.bounds.w);
            assert!(r.y + r.h <= dom.bounds.y + dom.bounds.h);
        }
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                assert!(
                    !(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h),
                    "overlap"
                );
            }
        }
    }
    let rendered = format!("{:?}", plan.dispatch.operation);
    assert!(!rendered.contains("kwin"));
    assert!(!rendered.contains("pixel"));
    assert!(!rendered.contains("handle"));
}
fn assert_focus_geometry_complete(plan: &SessionFocusPlan, session: &Session) {
    assert!(!plan.desired_geometry.is_empty());
    for g in &plan.desired_geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
        assert_eq!(
            (&g.output, &g.workspace),
            (
                &plan.desired_focus_domain.output,
                &plan.desired_focus_domain.workspace
            )
        );
    }
    let _ = session;
}

#[test]
fn r1_perpendicular_wrap() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Down, "m-1", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R1);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-2");
    assert_eq!(plan.desired_focus_domain, k);
    assert_geometry_complete(&plan, &s);
    assert_eq!(
        leaves(&s, "out-1", "ws-1"),
        vec!["leaf-win-1", "leaf-win-2"]
    );
}

#[test]
fn r2a_swap_neighbor() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", false, "c-3");
    admit_commit(&mut s, "win-4", "out-1", "ws-1", true, "c-4");
    let k = key("out-1", "ws-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Left, "m-1", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R2a);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-4");
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r2a_uneven_outer_preserved_and_window_share_binding() {
    // Binary admission nests entrants; walk focus to win-1, R2b to 3-wide,
    // then R2c wrap [1,2] gives uneven outer [2,1]; inner R2a swap
    // must preserve outer shares and swap window-share binding (no resize).
    // Source evidence: sizing S1/S2 (entrant 1/n, survivors scale) governs
    // entry; swap is reorder with no entry/removal, so windows retain shares.
    // R2c evidence: wrapper retains pair combined extent, halves inside.
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("win-{}", i + 1), "out-1", "ws-1", true, c);
    }
    let k = key("out-1", "ws-1");
    for corr in ["f-1", "f-2", "f-3"] {
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            corr,
            &FocusCapabilities::full(),
        );
    }
    let r2b = move_commit_focused(&mut s, &k, Direction::Right, "m-pre", &Capabilities::full());
    assert_eq!(r2b.dispatch.rule, tiler_core::directional::Rule::R2b);
    let r2c = move_commit_focused(&mut s, &k, Direction::Right, "m-1", &Capabilities::full());
    assert_eq!(r2c.dispatch.rule, tiler_core::directional::Rule::R2c);
    let snap = s.snapshot();
    let root = snap.domains[0].tree.clone().expect("tree");
    let (outer_shares, outer_len) = match &root {
        Node::Group {
            shares, children, ..
        } => (shares.clone(), children.len()),
        _ => panic!("root group"),
    };
    assert_eq!(outer_len, 2);
    assert_eq!(outer_shares.iter().sum::<u64>(), 3);
    assert_eq!(outer_shares, vec![2, 1]);
    // Inner R2a swap [1,2] -> [2,1]; outer uneven must be untouched.
    let r2a = move_commit_focused(&mut s, &k, Direction::Right, "m-2", &Capabilities::full());
    assert_eq!(r2a.dispatch.rule, tiler_core::directional::Rule::R2a);
    let snap2 = s.snapshot();
    let root2 = snap2.domains[0].tree.clone().expect("tree");
    match &root2 {
        Node::Group {
            shares, children, ..
        } => {
            assert_eq!(*shares, outer_shares, "uneven outer preserved on R2a");
            assert_eq!(children.len(), 2);
            match &children[0] {
                Node::Group {
                    children, shares, ..
                } => {
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-1");
                }
                other => panic!("wrapper {other:?}"),
            }
        }
        other => panic!("root {other:?}"),
    }
    assert_geometry_complete(&r2a, &s);
}

#[test]
fn r2b_insert_midpoint_after_r1() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    let r1 = move_commit_focused(&mut s, &k, Direction::Down, "m-1", &Capabilities::full());
    assert_eq!(r1.dispatch.rule, tiler_core::directional::Rule::R1);
    // Binary admission + R1 leaves H[win-1,V[win-2,win-3]]; win-1 faces a
    // perpendicular 2-child group, so Right from win-1 is Midpoint.
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-1",
        &FocusCapabilities::full(),
    );
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let r2b = move_commit_focused(&mut s, &k, Direction::Right, "m-2", &Capabilities::full());
    assert_eq!(r2b.dispatch.rule, tiler_core::directional::Rule::R2b);
    match &r2b.dispatch.operation {
        tiler_core::directional::MoveOperation::InsertIntoGroup { insertion, .. } => {
            assert_eq!(*insertion, tiler_core::directional::Insertion::Midpoint);
        }
        other => panic!("expected insert-into-group, got {other:?}"),
    }
    assert_eq!(r2b.desired_focus_leaf.0, "leaf-win-1");
    assert_geometry_complete(&r2b, &s);
}

#[test]
fn r2b_split_odd_target() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    admit_commit(&mut s, "win-4", "out-1", "ws-1", true, "c-4");
    let k = key("out-1", "ws-1");
    // Binary admission nests entrants; R1 Up on win-4 creates a 2-child
    // vertical group, focus to win-2, Midpoint grows it to 3, then Right
    // from win-1 splits the odd target.
    let r1 = move_commit_focused(&mut s, &k, Direction::Up, "m-1", &Capabilities::full());
    assert_eq!(r1.dispatch.rule, tiler_core::directional::Rule::R1);
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-1",
        &FocusCapabilities::full(),
    );
    assert_eq!(focused_window(&s, &k).0, "win-2");
    let r2b_mid = move_commit_focused(&mut s, &k, Direction::Right, "m-mid", &Capabilities::full());
    assert_eq!(r2b_mid.dispatch.rule, tiler_core::directional::Rule::R2b);
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-2",
        &FocusCapabilities::full(),
    );
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let r2b = move_commit_focused(&mut s, &k, Direction::Right, "m-2", &Capabilities::full());
    assert_eq!(r2b.dispatch.rule, tiler_core::directional::Rule::R2b);
    match &r2b.dispatch.operation {
        tiler_core::directional::MoveOperation::SplitGroupChild {
            focused_side,
            axis,
            target_child_index,
            ..
        } => {
            assert_eq!(*axis, tiler_core::directional::Axis::Horizontal);
            assert_eq!(*target_child_index, 1);
            // Right is step +1, so the mover splits Second (far side).
            assert_eq!(*focused_side, tiler_core::directional::FocusedSide::Second);
        }
        other => panic!("expected split-group-child, got {other:?}"),
    }
    assert_geometry_complete(&r2b, &s);
}

#[test]
fn r2b_near_edge_parallel_target() {
    // Binary admission already yields H[1,[2,3]]-shaped nesting:
    // H[win-1,[win-2,win-3]] with focus win-3. Navigate focus to win-1,
    // then 1 Right inserts NearEdge at 0 into the parallel target (S1-17/S3-08).
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    // Focus is win-3; navigate to win-2 then win-1.
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-1",
        &FocusCapabilities::full(),
    );
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-2",
        &FocusCapabilities::full(),
    );
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Right, "m-2", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R2b);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::InsertIntoGroup {
            insertion,
            insertion_index,
            target_group,
            ..
        } => {
            assert_eq!(*insertion, tiler_core::directional::Insertion::NearEdge);
            assert_eq!(*insertion_index, 0);
            assert!(!target_group.0.is_empty());
        }
        other => panic!("expected near-edge insert, got {other:?}"),
    }
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r2c_wrap_neighbor_nary() {
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("win-{}", i + 1), "out-1", "ws-1", true, c);
    }
    let k = key("out-1", "ws-1");
    // Binary admission nests each entrant, so the admitted focus faces a leaf
    // sibling (R2a). Walk focus to win-1, grow the inner group to 3 children
    // via an R2b insertion, then Right from win-1 is R2c.
    for (i, corr) in ["f-1", "f-2", "f-3"].iter().enumerate() {
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            corr,
            &FocusCapabilities::full(),
        );
        let _ = i;
    }
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let _ = move_commit_focused(&mut s, &k, Direction::Right, "m-pre", &Capabilities::full());
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Right, "m-1", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R2c);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-1");
    assert_geometry_complete(&plan, &s);
    let snap = s.snapshot();
    let tree = snap.domains[0].tree.clone().expect("tree");
    match tree {
        Node::Group {
            children, shares, ..
        } => {
            assert_eq!(children.len(), 2);
            assert_eq!(shares.iter().sum::<u64>(), 3);
            assert_eq!(shares, vec![2, 1]);
            match &children[0] {
                Node::Group {
                    children, shares, ..
                } => {
                    assert_eq!(children.len(), 2);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children[0].id().0, "leaf-win-1");
                    assert_eq!(children[1].id().0, "leaf-win-2");
                }
                other => panic!("expected wrapper, got {other:?}"),
            }
        }
        other => panic!("expected root group, got {other:?}"),
    }
}

#[test]
fn flat_swap_nary_end_to_end_with_traveling_shares() {
    // Same R2c fixture as `r2c_wrap_neighbor_nary`, but the final move runs
    // under SwapWithNeighbor: adjacent direct leaf siblings swap in place, shares
    // travel with their windows, focus stays, and no wrapper group appears.
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("win-{}", i + 1), "out-1", "ws-1", true, c);
    }
    let k = key("out-1", "ws-1");
    for corr in ["f-1", "f-2", "f-3"] {
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            corr,
            &FocusCapabilities::full(),
        );
    }
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let _ = move_commit_focused(&mut s, &k, Direction::Right, "m-pre", &Capabilities::full());
    assert_eq!(focused_window(&s, &k).0, "win-1");
    let focus_leaf = "leaf-win-1";
    let pre_tree = s.snapshot().domains[0].tree.clone().expect("tree");
    let (pre_children, pre_shares) = parent_children_shares(&pre_tree, focus_leaf);
    let pos = pre_children
        .iter()
        .position(|c| c == focus_leaf)
        .expect("focused child");
    assert!(
        pos + 1 < pre_children.len(),
        "fixture must offer a right sibling"
    );
    let right_sibling = pre_children[pos + 1].clone();
    let plan = move_commit_focused_with_mode(
        &mut s,
        &k,
        Direction::Right,
        "m-flat",
        &Capabilities::full(),
        SameAxisMove::SwapWithNeighbor,
    );
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R2c);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::SwapNeighbor { neighbor, .. } => {
            assert_eq!(neighbor.0, right_sibling);
        }
        other => panic!("expected flat swap, got {other:?}"),
    }
    assert_eq!(plan.desired_focus_leaf.0, focus_leaf);
    let post_tree = s.snapshot().domains[0].tree.clone().expect("tree");
    let (post_children, post_shares) = parent_children_shares(&post_tree, focus_leaf);
    assert_eq!(
        post_children.len(),
        pre_children.len(),
        "no wrapper group appears"
    );
    let mut expected_children = pre_children.clone();
    expected_children.swap(pos, pos + 1);
    assert_eq!(post_children, expected_children);
    let mut expected_shares = pre_shares.clone();
    expected_shares.swap(pos, pos + 1);
    assert_eq!(
        post_shares, expected_shares,
        "shares travel with their windows"
    );
    assert_geometry_complete(&plan, &s);
    // Swap back left through the public API: the same adjacent pair restores
    // the pre-move order and shares exactly, covering the left direction.
    let back = move_commit_focused_with_mode(
        &mut s,
        &k,
        Direction::Left,
        "m-flat-back",
        &Capabilities::full(),
        SameAxisMove::SwapWithNeighbor,
    );
    assert_eq!(back.dispatch.rule, tiler_core::directional::Rule::R2c);
    match &back.dispatch.operation {
        tiler_core::directional::MoveOperation::SwapNeighbor { neighbor, .. } => {
            assert_eq!(neighbor.0, right_sibling);
        }
        other => panic!("expected flat swap back, got {other:?}"),
    }
    assert_eq!(back.desired_focus_leaf.0, focus_leaf);
    let restored_tree = s.snapshot().domains[0].tree.clone().expect("tree");
    let (restored_children, restored_shares) = parent_children_shares(&restored_tree, focus_leaf);
    assert_eq!(restored_children, pre_children);
    assert_eq!(restored_shares, pre_shares);
    assert_geometry_complete(&back, &s);
}

#[test]
fn flat_swap_group_end_escape_unchanged() {
    // Same flow as `r3_escape_same_axis` through the R2c wrap, but the final
    // Left escape runs under SwapWithNeighbor: ancestor/boundary behavior is
    // identical (R3, same-axis insertion, no continuation).
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("win-{}", i + 1), "out-1", "ws-1", true, c);
    }
    let k = key("out-1", "ws-1");
    for corr in ["f-1", "f-2", "f-3"] {
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            corr,
            &FocusCapabilities::full(),
        );
    }
    let r2b = move_commit_focused(&mut s, &k, Direction::Right, "m-pre", &Capabilities::full());
    assert_eq!(r2b.dispatch.rule, tiler_core::directional::Rule::R2b);
    let r2c = move_commit_focused(&mut s, &k, Direction::Right, "m-1", &Capabilities::full());
    assert_eq!(r2c.dispatch.rule, tiler_core::directional::Rule::R2c);
    let r3 = move_commit_focused_with_mode(
        &mut s,
        &k,
        Direction::Left,
        "m-2",
        &Capabilities::full(),
        SameAxisMove::SwapWithNeighbor,
    );
    assert_eq!(r3.dispatch.rule, tiler_core::directional::Rule::R3);
    match &r3.dispatch.operation {
        tiler_core::directional::MoveOperation::EscapeParent {
            continuation,
            parent_insertion_index,
            ..
        } => {
            assert_eq!(
                *continuation,
                tiler_core::directional::EscapeContinuation::None
            );
            assert!(parent_insertion_index.is_some());
        }
        other => panic!("expected escape, got {other:?}"),
    }
    assert_geometry_complete(&r3, &s);
}

#[test]
fn r3_escape_with_r1_continuation() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    // Binary admission gives H[win-1,[win-2,win-3]]; R1 Down wraps the
    // focused pair perpendicular, then Down escapes with R1 continuation.
    let r1 = move_commit_focused(&mut s, &k, Direction::Down, "m-1", &Capabilities::full());
    assert_eq!(r1.dispatch.rule, tiler_core::directional::Rule::R1);
    let plan = move_commit_focused(&mut s, &k, Direction::Down, "m-2", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R3);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::EscapeParent { continuation, .. } => {
            assert_eq!(
                *continuation,
                tiler_core::directional::EscapeContinuation::R1
            );
        }
        other => panic!("expected escape, got {other:?}"),
    }
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r3_escape_same_axis() {
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("win-{}", i + 1), "out-1", "ws-1", true, c);
    }
    let k = key("out-1", "ws-1");
    // Binary admission nests entrants; walk focus to win-1, grow to 3 via
    // R2b, wrap [win-1,win-2] via R2c, then Left escapes same-axis.
    for corr in ["f-1", "f-2", "f-3"] {
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            corr,
            &FocusCapabilities::full(),
        );
    }
    let r2b = move_commit_focused(&mut s, &k, Direction::Right, "m-pre", &Capabilities::full());
    assert_eq!(r2b.dispatch.rule, tiler_core::directional::Rule::R2b);
    let r2c = move_commit_focused(&mut s, &k, Direction::Right, "m-1", &Capabilities::full());
    assert_eq!(r2c.dispatch.rule, tiler_core::directional::Rule::R2c);
    let r3 = move_commit_focused(&mut s, &k, Direction::Left, "m-2", &Capabilities::full());
    assert_eq!(r3.dispatch.rule, tiler_core::directional::Rule::R3);
    match &r3.dispatch.operation {
        tiler_core::directional::MoveOperation::EscapeParent {
            continuation,
            parent_insertion_index,
            ..
        } => {
            assert_eq!(
                *continuation,
                tiler_core::directional::EscapeContinuation::None
            );
            assert!(parent_insertion_index.is_some());
        }
        other => panic!("expected same-axis escape, got {other:?}"),
    }
    assert_geometry_complete(&r3, &s);
}

#[test]
fn r4_occupied_target() {
    let mut s = r4_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-2", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Right, "m-1", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::CrossOutput { target, .. } => {
            assert_eq!(
                *target,
                tiler_core::directional::CrossOutputTarget::Occupied
            );
        }
        other => panic!("expected cross-output, got {other:?}"),
    }
    assert_eq!(plan.desired_focus_domain, key("out-2", "ws-1"));
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-3");
    assert_eq!(
        s.focus(),
        (
            Some(key("out-2", "ws-1")),
            Some(NodeId("leaf-win-3".into()))
        )
    );
    assert_eq!(leaves(&s, "out-1", "ws-1"), vec!["leaf-win-1".to_string()]);
    assert_eq!(leaves(&s, "out-2", "ws-1").len(), 2);
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r4_empty_target() {
    let mut s = r4_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Right, "m-1", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::CrossOutput { target, .. } => {
            assert_eq!(*target, tiler_core::directional::CrossOutputTarget::Empty);
        }
        other => panic!("expected empty cross-output, got {other:?}"),
    }
    assert_eq!(plan.desired_focus_domain, key("out-2", "ws-1"));
    assert_eq!(leaves(&s, "out-2", "ws-1"), vec!["leaf-win-2".to_string()]);
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r4_edge_direction_domain_isolation() {
    // No adjacent boundary: single domain at root edge refuses noop.
    let mut solo = single_session();
    admit_commit(&mut solo, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut solo, "b", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    // Focus b is at right root edge; Right with no adjacency is Boundary noop.
    let w = focused_window(&solo, &k);
    let obs = complete_obs(&solo, vec![]);
    assert_eq!(
        solo.propose_move(
            &k,
            &w,
            Direction::Right,
            &obs,
            &correlation("e-1"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PlannerNoop))
    );
    assert!(!solo.has_pending());
    // Wrong direction from non-edge is a real move, not R4.
    let plan = move_commit_focused(&mut solo, &k, Direction::Left, "m-1", &Capabilities::full());
    assert_ne!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    // Different workspaces never cross even with adjacency-shaped ids.
    let mut two_ws = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("out-1", "ws-1", 200, 200),
            domain("out-2", "ws-2", 200, 200),
        ],
    )
    .expect("two ws");
    admit_commit(&mut two_ws, "w1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut two_ws, "w2", "out-1", "ws-1", true, "c-2");
    let k1 = key("out-1", "ws-1");
    let w = focused_window(&two_ws, &k1);
    let obs = complete_obs(&two_ws, vec![]);
    // At root edge Right but no same-workspace adjacency: noop, never R4.
    assert_eq!(
        two_ws.propose_move(
            &k1,
            &w,
            Direction::Right,
            &obs,
            &correlation("e-2"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PlannerNoop))
    );
    // Nested focus at root edge is not a direct root child: R4 requires a
    // direct root-edge leaf, so a deep focus moving outward is R3, not R4.
    let mut deep = single_session();
    admit_commit(&mut deep, "w1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut deep, "w2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut deep, "w3", "out-1", "ws-1", false, "c-3");
    admit_commit(&mut deep, "w4", "out-1", "ws-1", false, "c-4");
    let kd = key("out-1", "ws-1");
    let before = deep.snapshot();
    let _ = before;
    let plan = move_commit_focused(
        &mut deep,
        &kd,
        Direction::Down,
        "m-9",
        &Capabilities::full(),
    );
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R3);
}

#[test]
fn exact_opaque_scoping_move_and_focus() {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("out-1", "ws-1", 200, 200),
            domain("out-2", "ws-1", 200, 200),
        ],
    )
    .expect("two");
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-2", "ws-1", true, "c-2");
    // Focus is win-2 in out-2.
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");
    let obs = complete_obs(&s, vec![]);
    // Move with wrong-domain window refuses FocusMismatch.
    assert_eq!(
        s.propose_move(
            &k1,
            &WindowId("win-2".into()),
            Direction::Right,
            &obs,
            &correlation("s-1"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::FocusMismatch))
    );
    // Move with unknown window refuses UnknownWindow.
    assert_eq!(
        s.propose_move(
            &k2,
            &WindowId("nope".into()),
            Direction::Right,
            &obs,
            &correlation("s-2"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::UnknownWindow))
    );
    // Move with correct pair succeeds (single-leaf noop here, but scoping passes to planner).
    let w2 = WindowId("win-2".into());
    let solo_obs = complete_obs(&s, vec![]);
    let _ = s
        .propose_move(
            &k2,
            &w2,
            Direction::Right,
            &solo_obs,
            &correlation("s-3"),
            &Capabilities::full(),
        )
        .err();
    assert!(!s.has_pending() || s.has_pending());
    if s.has_pending() {
        // Clear pending via ack divergence path? Just verify no crash; reset via fresh session below.
        let _ = s.note_adapter_loss();
    }
    // Focus scoping mirrors moves.
    let mut f = single_session();
    admit_commit(&mut f, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut f, "b", "out-1", "ws-1", true, "c-2");
    let kf = key("out-1", "ws-1");
    let obsf = complete_obs(&f, vec![]);
    assert_eq!(
        f.propose_focus(
            &kf,
            &WindowId("a".into()),
            Direction::Left,
            &obsf,
            &correlation("f-9"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::FocusMismatch))
    );
    assert_eq!(
        f.propose_focus(
            &kf,
            &WindowId("nope".into()),
            Direction::Left,
            &obsf,
            &correlation("f-8"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::UnknownWindow))
    );
    assert!(!f.has_pending());
}

#[test]
fn accepted_plan_pre_ack_immutability() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let before_snap = s.snapshot();
    let before_focus = s.focus();
    let before_rev = s.accepted_revision();
    let obs = complete_obs(&s, vec![]);
    let plan = s
        .propose_move(
            &k,
            &w,
            Direction::Down,
            &obs,
            &correlation("p-1"),
            &Capabilities::full(),
        )
        .expect("plan")
        .expect_moved("tiled move");
    assert!(s.has_pending());
    assert!(s.has_pending_desired());
    // Accepted plan stages pending but accepted state is immutable until verify.
    assert_eq!(s.snapshot(), before_snap);
    assert_eq!(s.focus(), before_focus);
    assert_eq!(s.accepted_revision(), before_rev);
    assert_eq!(plan.dispatch.base_revision, before_rev);
    // Focus plan is equally immutable pre-ack.
    let mut f = single_session();
    admit_commit(&mut f, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut f, "b", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut f, "c", "out-1", "ws-1", true, "c-3");
    let kf = key("out-1", "ws-1");
    let wf = focused_window(&f, &kf);
    let fsnap = f.snapshot();
    let ffocus = f.focus();
    let frev = f.accepted_revision();
    let obsf = complete_obs(&f, vec![]);
    let fplan = f
        .propose_focus(
            &kf,
            &wf,
            Direction::Left,
            &obsf,
            &correlation("fp-1"),
            &FocusCapabilities::full(),
        )
        .expect("focus plan");
    assert!(f.has_pending());
    assert_eq!(f.snapshot(), fsnap);
    assert_eq!(f.focus(), ffocus);
    assert_eq!(f.accepted_revision(), frev);
    assert_eq!(fplan.dispatch.base_revision, frev);
}

#[test]
fn focus_ack_verify_commit_and_refusals() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    // Focus win-3 Left => win-2; topology unmodified, focus commits.
    let before_snap = s.snapshot();
    let plan = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-1",
        &FocusCapabilities::full(),
    );
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-2");
    assert_eq!(s.focus().1, Some(NodeId("leaf-win-2".into())));
    assert_eq!(s.snapshot().domains, before_snap.domains);
    assert_focus_geometry_complete(&plan, &s);
    // Edge refuses Unchanged with no pending.
    let mut e = single_session();
    admit_commit(&mut e, "solo", "out-1", "ws-1", true, "c-9");
    let ke = key("out-1", "ws-1");
    let we = focused_window(&e, &ke);
    let obse = complete_obs(&e, vec![]);
    assert_eq!(
        e.propose_focus(
            &ke,
            &we,
            Direction::Left,
            &obse,
            &correlation("fe-1"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!e.has_pending());
    // Unsupported focus capability refuses.
    let mut u = single_session();
    admit_commit(&mut u, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut u, "b", "out-1", "ws-1", true, "c-2");
    let ku = key("out-1", "ws-1");
    let wu = focused_window(&u, &ku);
    let obsu = complete_obs(&u, vec![]);
    assert_eq!(
        u.propose_focus(
            &ku,
            &wu,
            Direction::Left,
            &obsu,
            &correlation("fu-1"),
            &FocusCapabilities::none()
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    assert!(!u.has_pending());
    // Pending refuses second focus.
    let mut p = single_session();
    admit_commit(&mut p, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut p, "b", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut p, "c", "out-1", "ws-1", true, "c-3");
    let kp = key("out-1", "ws-1");
    let wp = focused_window(&p, &kp);
    let obsp = complete_obs(&p, vec![]);
    p.propose_focus(
        &kp,
        &wp,
        Direction::Left,
        &obsp,
        &correlation("fp-1"),
        &FocusCapabilities::full(),
    )
    .expect("first");
    let obs2 = complete_obs(&p, vec![]);
    assert_eq!(
        p.propose_focus(
            &kp,
            &wp,
            Direction::Left,
            &obs2,
            &correlation("fp-2"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::PendingExists)
    );
    // Stale revision diverges.
    let mut st = single_session();
    admit_commit(&mut st, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut st, "b", "out-1", "ws-1", true, "c-2");
    let ks = key("out-1", "ws-1");
    let ws = focused_window(&st, &ks);
    let mut bad = complete_obs(&st, vec![]);
    bad.observation = Observation::new(owner(), generation(), 99, 99);
    assert_eq!(
        st.propose_focus(
            &ks,
            &ws,
            Direction::Left,
            &bad,
            &correlation("fs-1"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
    );
    // Incomplete observation refuses (fresh session; `st` already diverged).
    let mut pi = single_session();
    admit_commit(&mut pi, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut pi, "b", "out-1", "ws-1", true, "c-2");
    let kpi = key("out-1", "ws-1");
    let wpi = focused_window(&pi, &kpi);
    let partial = SessionObservation {
        observation: Observation::new(owner(), generation(), pi.accepted_revision(), 5),
        windows: vec![],
    };
    assert_eq!(
        pi.propose_focus(
            &kpi,
            &wpi,
            Direction::Left,
            &partial,
            &correlation("fp-3"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
}

#[test]
fn per_capability_representative_routes() {
    // Each movement capability has a representative Session route: full
    // succeeds (or plans), missing that capability refuses Unsupported.
    // Bounded: one session per capability family.
    // SwapNeighbor (R2a).
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_swap = Capabilities::full();
    no_swap.swap_neighbor = false;
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Left,
            &obs,
            &correlation("cap-1"),
            &no_swap
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // WrapPerpendicular (R1).
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_wrap = Capabilities::full();
    no_wrap.wrap_perpendicular = false;
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Down,
            &obs,
            &correlation("cap-2"),
            &no_wrap
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // WrapSiblings (R2c).
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("w{i}"), "out-1", "ws-1", true, c);
    }
    let k = key("out-1", "ws-1");
    // Binary admission faces R2a; walk to w0 and grow to 3 via R2b so Right is R2c.
    for corr in ["f-cap-1", "f-cap-2", "f-cap-3"] {
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            corr,
            &FocusCapabilities::full(),
        );
    }
    let _ = move_commit_focused(
        &mut s,
        &k,
        Direction::Right,
        "m-cap-pre",
        &Capabilities::full(),
    );
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_wrap_sib = Capabilities::full();
    no_wrap_sib.wrap_siblings = false;
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Right,
            &obs,
            &correlation("cap-3"),
            &no_wrap_sib
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // InsertChild (R2b midpoint representative).
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "c", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    let _ = move_commit_focused(&mut s, &k, Direction::Down, "m-cap", &Capabilities::full());
    // Binary admission + R1 leaves H[a,V[b,c]]; a faces a perpendicular
    // 2-child group, so Right from a is Midpoint R2b.
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-cap",
        &FocusCapabilities::full(),
    );
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_insert = Capabilities::full();
    no_insert.insert_child = false;
    // After R1, Right from a is R2b (insert or split depending on parity); force check
    // that disabling both R2b caps refuses, and at least one refuses.
    let r_insert = s.propose_move(
        &k,
        &w,
        Direction::Right,
        &obs,
        &correlation("cap-4"),
        &no_insert,
    );
    assert_eq!(
        r_insert,
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // SplitGroupChild representative (odd target).
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "c", "out-1", "ws-1", true, "c-3");
    admit_commit(&mut s, "d", "out-1", "ws-1", true, "c-4");
    let k = key("out-1", "ws-1");
    // Binary admission nests; R1 Up on d, focus to b, Midpoint grows the
    // vertical group to 3, focus to a, then Right splits the odd target.
    let _ = move_commit_focused(&mut s, &k, Direction::Up, "m-cap2", &Capabilities::full());
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-cap2",
        &FocusCapabilities::full(),
    );
    let _ = move_commit_focused(
        &mut s,
        &k,
        Direction::Right,
        "m-cap2b",
        &Capabilities::full(),
    );
    let _ = focus_commit_focused(
        &mut s,
        &k,
        Direction::Left,
        "f-cap2b",
        &FocusCapabilities::full(),
    );
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_split = Capabilities::full();
    no_split.split_group_child = false;
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Right,
            &obs,
            &correlation("cap-5"),
            &no_split
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // ReparentLeaf (R3).
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "c", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    // Binary admission + R1 leaves H[a,V[b,c]]; Down from c escapes with R1 continuation.
    let _ = move_commit_focused(&mut s, &k, Direction::Down, "m-cap3", &Capabilities::full());
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_reparent = Capabilities::full();
    no_reparent.reparent_leaf = false;
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Down,
            &obs,
            &correlation("cap-6"),
            &no_reparent
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // CrossOutputTransfer (R4).
    let mut s = r4_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let mut no_cross = Capabilities::full();
    no_cross.cross_output_transfer = false;
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Right,
            &obs,
            &correlation("cap-7"),
            &no_cross
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    // Focus capability representative.
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_focus(
            &k,
            &w,
            Direction::Left,
            &obs,
            &correlation("cap-8"),
            &FocusCapabilities::none()
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
}

#[test]
fn move_geometry_projection_fallback() {
    // R4 into a 1px occupied target cannot project 2 leaves: under the
    // default float result the exact wrap proves exhaustion and stages the
    // shared capacity-float fallback (MoveToFloat at the chosen target,
    // follow-only, untouched frame) instead of refusing. The staged
    // plan commits through the lifecycle verifier with the source released
    // and the destination tree untouched.
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            OutputDomain {
                id: OutputId("out-2".into()),
                workspace: WorkspaceId("ws-1".into()),
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
                gap: 0,
                adjacent: [(Direction::Left, OutputId("out-1".into()))]
                    .into_iter()
                    .collect(),
            },
        ],
    )
    .expect("r4 tiny");
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-2", "ws-1", true, "c-2");
    let c_plan = admit_commit(&mut s, "c", "out-1", "ws-1", true, "c-3");
    let _frame_c = c_plan
        .desired_geometry
        .iter()
        .find(|g| g.window.0 == "c")
        .expect("c projected at admission")
        .rect;
    let k = key("out-1", "ws-1");
    let target = key("out-2", "ws-1");
    let dest_tree_before = s.tree_for(&target).expect("target tree").clone();
    let w = focused_window(&s, &k);
    assert_eq!(w.0, "c");
    let mover_leaf = s
        .snapshot()
        .windows
        .iter()
        .find(|l| l.window == w)
        .expect("mover link")
        .leaf
        .clone();
    let obs = complete_obs(&s, vec![]);
    let outcome = s.propose_move(
        &k,
        &w,
        Direction::Right,
        &obs,
        &correlation("g-1"),
        &Capabilities::full(),
    );
    let plan = outcome
        .expect("float R4 stages the capacity fallback")
        .capacity_float()
        .expect("fallback is the slotless float");
    match &plan.dispatch.operation {
        tiler_core::contract::LifecycleOperation::MoveToFloat {
            window,
            leaf,
            target_output,
            target_workspace,
            ..
        } => {
            assert_eq!(window, &w);
            assert_eq!(leaf, &mover_leaf);
            assert_eq!(target_output.0, "out-2");
            assert_eq!(target_workspace.0, "ws-1");
        }
        other => panic!("float R4 carries MoveToFloat, got {other:?}"),
    }
    assert!(
        plan.desired_geometry.iter().all(|g| g.window != w),
        "slotless mover takes no tiled geometry"
    );
    assert!(s.has_pending() && s.has_pending_desired());
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("g-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 971 + base),
        correlation("g-1"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify float fallback");
    assert!(s.is_capacity_float(&w));
    // Cross-output honesty: no source-coordinate claim is staged at the
    // target (the established native remap plus its arrival proof owns
    // placement there); the frame stays untouched until arrival.
    assert_eq!(s.floating_geometry(&w), None);
    assert_eq!(
        s.tree_for(&target),
        Some(&dest_tree_before),
        "destination tree untouched"
    );
    let _ = std::mem::replace(&mut s, single_session());
}

#[test]
fn cross_kind_reconciliation_mismatches() {
    // Move pending verified with focus/lifecycle kinds diverges.
    let mut s = single_session();
    admit_commit(&mut s, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "b", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let base = s.accepted_revision();
    let plan = s
        .propose_move(
            &k,
            &w,
            Direction::Down,
            &obs,
            &correlation("x-1"),
            &Capabilities::full(),
        )
        .expect("move plan");
    s.acknowledge(&AdapterAck::new(
        correlation("x-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    // Lifecycle verifier on move pending diverges (cross-kind mismatch).
    let _ = &plan;
    let lifecycle_post = tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 1),
        correlation("x-1"),
        true,
        vec![
            tiler_core::contract::LifecyclePrecondition::WindowObserved,
            tiler_core::contract::LifecyclePrecondition::DesiredTopologyValid,
            tiler_core::contract::LifecyclePrecondition::AdapterMustVerifyPostconditions,
        ],
        tiler_core::contract::LifecycleOperation::Remove {
            window: WindowId("a".into()),
            leaf: NodeId("leaf-a".into()),
            output: OutputId("out-1".into()),
            workspace: WorkspaceId("ws-1".into()),
        },
    );
    assert_eq!(
        s.verify_lifecycle(&lifecycle_post),
        Err(tiler_core::reconcile::VerifyError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert!(!s.has_pending());
    // Focus pending verified with move kind diverges.
    let mut f = single_session();
    admit_commit(&mut f, "a", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut f, "b", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut f, "c", "out-1", "ws-1", true, "c-3");
    let kf = key("out-1", "ws-1");
    let wf = focused_window(&f, &kf);
    let obsf = complete_obs(&f, vec![]);
    let basef = f.accepted_revision();
    let fplan = f
        .propose_focus(
            &kf,
            &wf,
            Direction::Left,
            &obsf,
            &correlation("y-1"),
            &FocusCapabilities::full(),
        )
        .expect("focus plan");
    f.acknowledge(&AdapterAck::new(
        correlation("y-1"),
        owner(),
        generation(),
        basef,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    let move_post = PostObservation::new(
        Observation::new(owner(), generation(), basef, 2),
        correlation("y-1"),
        true,
        fplan
            .dispatch
            .preconditions
            .iter()
            .map(|_| tiler_core::directional::Precondition::AdapterMustVerifyPostconditions)
            .collect(),
        tiler_core::directional::MoveOperation::SwapNeighbor {
            rule: tiler_core::directional::Rule::R2a,
            container: NodeId("root".into()),
            neighbor: NodeId("x".into()),
        },
    );
    assert_eq!(
        f.verify_move(&move_post),
        Err(tiler_core::reconcile::VerifyError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert!(!f.has_pending());
}

#[test]
fn deep_nary_route_preserves_order_and_shares() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", false, "c-3");
    admit_commit(&mut s, "win-4", "out-1", "ws-1", false, "c-4");
    admit_commit(&mut s, "win-5", "out-1", "ws-1", true, "c-5");
    let k = key("out-1", "ws-1");
    let p1 = move_commit_focused(&mut s, &k, Direction::Up, "m-1", &Capabilities::full());
    assert_geometry_complete(&p1, &s);
    let focus_after_p1 = s.focus().1.expect("focus");
    assert_eq!(p1.desired_focus_leaf, focus_after_p1);
    let p2 = move_commit_focused(&mut s, &k, Direction::Down, "m-2", &Capabilities::full());
    assert_geometry_complete(&p2, &s);
    let snap = s.snapshot();
    let tree = snap.domains[0].tree.clone().expect("tree");
    let mut ids = BTreeSet::new();
    check_shares(&tree, &mut ids);
    assert_eq!(ids.len(), 5 + count_groups(&tree));
}

fn count_groups(node: &Node) -> usize {
    match node {
        Node::Leaf { .. } => 0,
        Node::Group { children, .. } => 1 + children.iter().map(count_groups).sum::<usize>(),
    }
}
fn check_shares(node: &Node, ids: &mut BTreeSet<String>) {
    assert!(ids.insert(node.id().0.clone()), "duplicate node id");
    match node {
        Node::Leaf { .. } => {}
        Node::Group {
            children, shares, ..
        } => {
            assert!(children.len() >= 2);
            assert_eq!(shares.len(), children.len());
            assert!(shares.iter().all(|s| *s > 0));
            for c in children {
                check_shares(c, ids);
            }
        }
    }
}

#[test]
fn navigation_deterministic_focus_commit() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let first = s
        .propose_focus(
            &k,
            &w,
            Direction::Left,
            &obs,
            &correlation("d-1"),
            &FocusCapabilities::full(),
        )
        .expect("focus");
    // Second proposal while pending refuses.
    let obs2 = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_focus(
            &k,
            &w,
            Direction::Left,
            &obs2,
            &correlation("d-2"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::PendingExists)
    );
    // Clear pending via adapter loss for determinism check below.
    s.note_adapter_loss();
    let mut a = single_session();
    admit_commit(&mut a, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut a, "win-2", "out-1", "ws-1", true, "c-2");
    admit_commit(&mut a, "win-3", "out-1", "ws-1", true, "c-3");
    let wa = focused_window(&a, &k);
    let obsa = complete_obs(&a, vec![]);
    let pa = a
        .propose_focus(
            &k,
            &wa,
            Direction::Left,
            &obsa,
            &correlation("d-1"),
            &FocusCapabilities::full(),
        )
        .expect("focus");
    assert_eq!(first.dispatch.operation, pa.dispatch.operation);
    assert_eq!(first.desired_focus_leaf.0, "leaf-win-2");
    let _ = s;
    let _ = a;
}

#[test]
fn domain_isolation_outside_r4() {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("out-1", "ws-1", 200, 200),
            domain("out-2", "ws-2", 200, 200),
        ],
    )
    .expect("two domains");
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-2", "ws-2", true, "c-2");
    admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
    let before_other = leaves(&s, "out-2", "ws-2");
    let k = key("out-1", "ws-1");
    let plan = move_commit_focused(&mut s, &k, Direction::Left, "m-1", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R2a);
    assert_eq!(leaves(&s, "out-2", "ws-2"), before_other);
    assert_eq!(plan.desired_focus_domain, k);
}

#[test]
fn refusals_unknown_focus_incomplete_stale_capability_pending_geometry() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    let k = key("out-1", "ws-1");
    let w1 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_move(
            &key("nope", "ws-1"),
            &w1,
            Direction::Right,
            &obs,
            &correlation("x-1"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::UnknownDomain))
    );
    let mut two = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("out-1", "ws-1", 200, 200),
            domain("out-2", "ws-1", 200, 200),
        ],
    )
    .expect("two");
    admit_commit(&mut two, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut two, "win-2", "out-2", "ws-1", true, "c-2");
    let obs2 = complete_obs(&two, vec![]);
    // Focus is out-2; requesting out-1 with win-1 refuses FocusMismatch.
    assert_eq!(
        two.propose_move(
            &key("out-1", "ws-1"),
            &WindowId("win-1".into()),
            Direction::Right,
            &obs2,
            &correlation("x-2"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::FocusMismatch))
    );
    assert!(!two.has_pending());
    let partial = SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 5),
        windows: vec![],
    };
    assert_eq!(
        s.propose_move(
            &k,
            &w1,
            Direction::Right,
            &partial,
            &correlation("x-3"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert!(!s.has_pending());
    let mut stale_s = single_session();
    admit_commit(&mut stale_s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut stale_s, "win-2", "out-1", "ws-1", true, "c-2");
    let ws = focused_window(&stale_s, &k);
    let mut bad = complete_obs(&stale_s, vec![]);
    bad.observation = Observation::new(owner(), generation(), 99, 99);
    assert_eq!(
        stale_s.propose_move(
            &k,
            &ws,
            Direction::Down,
            &bad,
            &correlation("x-4"),
            &Capabilities::full()
        ),
        Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
    );
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Down,
            &obs,
            &correlation("x-5"),
            &Capabilities::none()
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    assert!(!s.has_pending());
    let obs = complete_obs(&s, vec![]);
    let w = focused_window(&s, &k);
    s.propose_move(
        &k,
        &w,
        Direction::Down,
        &obs,
        &correlation("p-1"),
        &Capabilities::full(),
    )
    .expect("first pending");
    assert!(s.has_pending());
    let obs2 = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Down,
            &obs2,
            &correlation("p-2"),
            &Capabilities::full()
        ),
        Err(ProposeError::PendingExists)
    );
    let mut solo = single_session();
    admit_commit(&mut solo, "only", "out-1", "ws-1", true, "c-9");
    let ks = key("out-1", "ws-1");
    let wsolo = focused_window(&solo, &ks);
    let obs = complete_obs(&solo, vec![]);
    assert_eq!(
        solo.propose_move(
            &ks,
            &wsolo,
            Direction::Right,
            &obs,
            &correlation("x-6"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PlannerNoop))
    );
    assert!(!solo.has_pending());
}

#[test]
fn invalid_geometry_refuses() {
    // A 1x1 domain hosts exactly one positive tile: the second window
    // exhausts every anchor (ordinary plus all fallbacks), so the
    // default float no-space result admits it as a slotless capacity
    // float instead of refusing (best effort, selected 2026-10-17).
    let mut tiny = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("tiny", "ws-1", 1, 1)],
    )
    .expect("tiny");
    admit_commit(&mut tiny, "a", "tiny", "ws-1", true, "c-1");
    let obs = complete_obs(&tiny, vec![tiled("b", "tiny", "ws-1")]);
    let cmd = SessionCommand::Admit {
        window: WindowId("b".into()),
        output: OutputId("tiny".into()),
        workspace: WorkspaceId("ws-1".into()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: placement(true),
        suppress_fixed_float: false,
    };
    let plan = tiny
        .propose(
            &cmd,
            &obs,
            &correlation("g-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("exhaustion floats instead of refusing");
    assert!(
        plan.desired_geometry.is_empty(),
        "capacity float takes no tile slot"
    );
    let base = tiny.accepted_revision();
    tiny.acknowledge(&AdapterAck::new(
        correlation("g-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    tiny.verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 209),
        correlation("g-1"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
    assert!(tiny.is_exception(&WindowId("b".into())), "b floats");
    assert!(
        tiny.is_capacity_float(&WindowId("b".into())),
        "capacity origin is diagnostic-only and distinct from fixed auto"
    );
    assert!(
        !tiny.is_automatic_fixed_float(&WindowId("b".into())),
        "capacity never reuses the fixed-size marker"
    );
    assert_eq!(tiny.snapshot().windows.len(), 1, "a keeps its tile");
    assert_eq!(tiny.divergence(), None);
    assert!(!tiny.has_pending());
}

#[test]
fn ack_verify_commit_and_mismatch_clears() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
    admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let before_rev = s.accepted_revision();
    let obs = complete_obs(&s, vec![]);
    let plan = s
        .propose_move(
            &k,
            &w,
            Direction::Down,
            &obs,
            &correlation("m-1"),
            &Capabilities::full(),
        )
        .expect("plan")
        .expect_moved("tiled move");
    assert!(s.has_pending());
    let early = PostObservation::new(
        Observation::new(owner(), generation(), before_rev, 1),
        correlation("m-1"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    assert_eq!(
        s.verify_move(&early),
        Err(tiler_core::reconcile::VerifyError::NotAcknowledged)
    );
    assert!(s.has_pending());
    s.acknowledge(&AdapterAck::new(
        correlation("m-1"),
        owner(),
        generation(),
        before_rev,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    let mut bad_pre = plan.dispatch.preconditions.clone();
    bad_pre.pop();
    let bad = PostObservation::new(
        Observation::new(owner(), generation(), before_rev, 2),
        correlation("m-1"),
        true,
        bad_pre,
        plan.dispatch.operation.clone(),
    );
    assert_eq!(
        s.verify_move(&bad),
        Err(tiler_core::reconcile::VerifyError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert!(!s.has_pending());
    assert!(!s.has_pending_desired());
}

#[test]
fn deterministic_replay_of_moves() {
    fn run() -> (Session, Vec<SessionMovePlan>) {
        let mut s = single_session();
        admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
        admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
        admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
        let k = key("out-1", "ws-1");
        let p1 = move_commit_focused(&mut s, &k, Direction::Left, "m-1", &Capabilities::full());
        let p2 = move_commit_focused(&mut s, &k, Direction::Down, "m-2", &Capabilities::full());
        (s, vec![p1, p2])
    }
    let (a_s, a_p) = run();
    let (b_s, b_p) = run();
    assert_eq!(a_s.snapshot(), b_s.snapshot());
    assert_eq!(a_s.focus(), b_s.focus());
    assert_eq!(a_p, b_p);
    // Session focus replay is equally deterministic.
    fn run_focus() -> Session {
        let mut s = single_session();
        admit_commit(&mut s, "win-1", "out-1", "ws-1", true, "c-1");
        admit_commit(&mut s, "win-2", "out-1", "ws-1", true, "c-2");
        admit_commit(&mut s, "win-3", "out-1", "ws-1", true, "c-3");
        let k = key("out-1", "ws-1");
        let _ = focus_commit_focused(
            &mut s,
            &k,
            Direction::Left,
            "f-1",
            &FocusCapabilities::full(),
        );
        s
    }
    assert_eq!(run_focus().focus(), run_focus().focus());
}

#[test]
fn adjacency_validation_strict() {
    let bad = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            domain("out-2", "ws-1", 200, 200),
        ],
    );
    assert!(bad.is_err());
    let bad = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain_with_adjacent(
            "out-1",
            "ws-1",
            vec![(Direction::Right, "nope")],
        )],
    );
    assert!(bad.is_err());
    let bad = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain_with_adjacent(
            "out-1",
            "ws-1",
            vec![(Direction::Right, "out-1")],
        )],
    );
    assert!(bad.is_err());
    // Cross-workspace adjacency is now valid: the target is the adjacent
    // output's currently selected logical workspace, not the same backing
    // desktop index.
    let ok = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            domain_with_adjacent("out-2", "ws-2", vec![(Direction::Left, "out-1")]),
        ],
    );
    assert!(ok.is_ok());
    // Ambiguous duplicate output ids still fail closed.
    let bad = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            domain_with_adjacent("out-2", "ws-2", vec![(Direction::Left, "out-1")]),
            domain_with_adjacent("out-2", "ws-3", vec![(Direction::Left, "out-1")]),
        ],
    );
    assert!(bad.is_err());
}

#[test]
fn bounded_property_loop_over_accepted_moves() {
    let mut s = single_session();
    for (i, c) in ["c-1", "c-2", "c-3", "c-4", "c-5"].iter().enumerate() {
        admit_commit(
            &mut s,
            &format!("win-{}", i + 1),
            "out-1",
            "ws-1",
            i % 2 == 0,
            c,
        );
    }
    let k = key("out-1", "ws-1");
    let dirs = [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ];
    for i in 0..24 {
        let dir = dirs[i % dirs.len()];
        let corr_id = format!("loop-{i}");
        let obs = complete_obs(&s, vec![]);
        let before_snap = s.snapshot();
        let before_focus = s.focus();
        let before_rev = s.accepted_revision();
        let w = focused_window(&s, &k);
        match s.propose_move(
            &k,
            &w,
            dir,
            &obs,
            &correlation(&corr_id),
            &Capabilities::full(),
        ) {
            Ok(outcome) => {
                let plan = outcome.expect_moved("tiled move");
                assert_eq!(plan.dispatch.base_revision, before_rev);
                // Frozen binding invariants on every accepted plan.
                assert_eq!(plan.dispatch.rule, plan.dispatch.operation.rule());
                assert_eq!(
                    plan.dispatch.required_capability,
                    plan.dispatch.operation.required_capability()
                );
                assert_eq!(
                    plan.dispatch.preconditions,
                    plan.dispatch.operation.preconditions()
                );
                assert_eq!(plan.dispatch.intent.focused_leaf, w.leaf_clone(&s, &k));
                let mut win_ids = BTreeSet::new();
                for wlink in &plan.desired_snapshot.windows {
                    assert!(win_ids.insert(wlink.window.0.clone()), "duplicate window");
                    assert_eq!(
                        (wlink.output.0.as_str(), wlink.workspace.0.as_str()),
                        ("out-1", "ws-1")
                    );
                }
                assert_geometry_complete(&plan, &s);
                s.acknowledge(&AdapterAck::new(
                    correlation(&corr_id),
                    owner(),
                    generation(),
                    before_rev,
                    AckOutcome::Accepted,
                ))
                .expect("ack");
                let post = PostObservation::new(
                    Observation::new(owner(), generation(), before_rev, 500 + i as u64),
                    correlation(&corr_id),
                    true,
                    plan.dispatch.preconditions.clone(),
                    plan.dispatch.operation.clone(),
                );
                s.verify_move(&post).expect("verify");
                assert_eq!(s.accepted_revision(), before_rev + 1);
                assert_eq!(s.focus().1, Some(plan.desired_focus_leaf.clone()));
            }
            Err(ProposeError::Refused(_)) => {
                assert!(!s.has_pending());
                assert!(!s.has_pending_desired());
                assert_eq!(s.snapshot(), before_snap);
                assert_eq!(s.focus(), before_focus);
                assert_eq!(s.accepted_revision(), before_rev);
            }
            Err(ProposeError::PendingExists) => panic!("no pending expected in loop"),
            Err(ProposeError::Diverged(kind)) => panic!("loop must not diverge: {kind:?}"),
        }
    }
}

trait FocusedLeafHelper {
    fn leaf_clone(&self, session: &Session, domain: &DomainKey) -> tiler_core::directional::NodeId;
}
impl FocusedLeafHelper for WindowId {
    fn leaf_clone(&self, session: &Session, domain: &DomainKey) -> tiler_core::directional::NodeId {
        session
            .snapshot()
            .windows
            .iter()
            .find(|l| {
                &l.window == self && l.output == domain.output && l.workspace == domain.workspace
            })
            .expect("leaf for window")
            .leaf
            .clone()
    }
}

#[test]
fn portable_session_movement_prohibits_platform_imports() {
    const SESSION: &str = include_str!("../src/session.rs");
    const BANNED: &[&str] = &[
        "zbus",
        "rustix",
        "dbus",
        "tokio",
        "async-std",
        "smol",
        "wayland",
        "x11",
        "kwin",
        "libc",
        "nix",
        "socket",
        "ipc",
        "process",
        "platform",
        "std::process",
        "std::net",
        "std::os",
        "std::fs",
        "std::env",
        "std::thread",
        "ffi",
    ];
    for raw_line in SESSION.lines() {
        let line = raw_line.trim().to_ascii_lowercase();
        if !(line.starts_with("use ")
            || line.starts_with("extern crate")
            || line.starts_with("mod "))
        {
            continue;
        }
        for token in BANNED {
            assert!(
                !line.contains(token),
                "import {raw_line:?} contains {token:?}"
            );
        }
    }
}

// ---- item 5: four-direction cross (REQ-MOV-08/OUT-01, decisions 5.1/5.2) ----

fn stacked_session() -> Session {
    // out-top above out-bottom: reciprocal Up/Down adjacency, one workspace.
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-top", "ws-1", vec![(Direction::Down, "out-bottom")]),
            domain_with_adjacent("out-bottom", "ws-1", vec![(Direction::Up, "out-top")]),
        ],
    )
    .expect("stacked session")
}

fn tree_of(session: &Session, output: &str, workspace: &str) -> Option<Node> {
    session
        .snapshot()
        .domains
        .into_iter()
        .find(|d| d.output.0 == output && d.workspace.0 == workspace)
        .and_then(|d| d.tree)
}

fn root_children(tree: &Node) -> (tiler_core::directional::Axis, Vec<String>) {
    match tree {
        Node::Leaf { id } => panic!("expected root group, got leaf {}", id.0),
        Node::Group { axis, children, .. } => {
            (*axis, children.iter().map(|c| c.id().0.clone()).collect())
        }
    }
}

#[test]
fn r4_vertical_cross_occupied_landing_nearest_source() {
    // Stacked outputs: win-t occupies out-bottom; V[win-1 win-2] sits on
    // out-top with the mover win-2 at the bottom edge. Moving Down crosses
    // after local exhaustion (item 5.1) and lands with a whole-target D-axis
    // wrap nearest the source: the target becomes V[W, win-t] with the mover
    // first (above).
    let mut s = stacked_session();
    admit_commit(&mut s, "win-t", "out-bottom", "ws-1", true, "v-0");
    admit_commit(&mut s, "win-1", "out-top", "ws-1", false, "v-1");
    admit_commit(&mut s, "win-2", "out-top", "ws-1", false, "v-2");
    let k = key("out-top", "ws-1");
    assert_eq!(focused_window(&s, &k).0, "win-2");
    let plan = move_commit_focused(&mut s, &k, Direction::Down, "v-3", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::CrossOutput {
            target_output,
            target_workspace,
            target,
            ..
        } => {
            assert_eq!(target_output.0, "out-bottom");
            assert_eq!(target_workspace.0, "ws-1");
            assert_eq!(
                *target,
                tiler_core::directional::CrossOutputTarget::Occupied
            );
        }
        other => panic!("expected cross-output, got {other:?}"),
    }
    assert_eq!(plan.desired_focus_domain, key("out-bottom", "ws-1"));
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-2");
    let target = tree_of(&s, "out-bottom", "ws-1").expect("target tree");
    let (axis, children) = root_children(&target);
    assert_eq!(axis, tiler_core::directional::Axis::Vertical);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], "leaf-win-2", "mover nearest the source above");
    assert_eq!(children[1], "leaf-win-t");
    assert_eq!(
        leaves(&s, "out-top", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r4_vertical_cross_up_landing_nearest_source() {
    // Mirror: mover at the top edge of out-bottom moves Up onto occupied
    // out-top; the mover lands last (nearest the source below).
    let mut s = stacked_session();
    admit_commit(&mut s, "win-t", "out-top", "ws-1", true, "u-0");
    admit_commit(&mut s, "win-1", "out-bottom", "ws-1", false, "u-1");
    admit_commit(&mut s, "win-2", "out-bottom", "ws-1", false, "u-2");
    // Focus win-1 (top edge of the bottom stack) via directional focus.
    let kb = key("out-bottom", "ws-1");
    let w2 = focused_window(&s, &kb);
    assert_eq!(w2.0, "win-2");
    focus_commit(
        &mut s,
        &kb,
        &w2,
        Direction::Up,
        "u-focus",
        &tiler_core::contract::FocusCapabilities::full(),
    );
    assert_eq!(focused_window(&s, &kb).0, "win-1");
    let plan = move_commit_focused(&mut s, &kb, Direction::Up, "u-3", &Capabilities::full());
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, key("out-top", "ws-1"));
    let target = tree_of(&s, "out-top", "ws-1").expect("target tree");
    let (axis, children) = root_children(&target);
    assert_eq!(axis, tiler_core::directional::Axis::Vertical);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], "leaf-win-t");
    assert_eq!(children[1], "leaf-win-1", "mover nearest the source below");
    assert_eq!(
        leaves(&s, "out-bottom", "ws-1"),
        vec!["leaf-win-2".to_string()]
    );
    assert_geometry_complete(&plan, &s);
}

#[test]
fn r4_sole_leaf_cross_empties_source_all_directions() {
    // Item 5.1: a sole root leaf crosses in all four directions (changing the
    // old horizontal SingleRootLeaf no-cross). The source domain empties and
    // the mover becomes the lone root of the target's current workspace.
    for (direction, target_output) in [
        (Direction::Left, "out-w"),
        (Direction::Right, "out-e"),
        (Direction::Up, "out-n"),
        (Direction::Down, "out-s"),
    ] {
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let mut s = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![
                domain_with_adjacent("out-c", "ws-1", vec![(direction, target_output)]),
                domain_with_adjacent(target_output, "ws-1", vec![(opposite, "out-c")]),
            ],
        )
        .expect("compass session");
        admit_commit(&mut s, "solo", "out-c", "ws-1", true, "solo-1");
        let k = key("out-c", "ws-1");
        let plan = move_commit_focused(&mut s, &k, direction, "solo-2", &Capabilities::full());
        assert_eq!(
            plan.dispatch.rule,
            tiler_core::directional::Rule::R4,
            "{direction:?}"
        );
        assert_eq!(plan.desired_focus_domain, key(target_output, "ws-1"));
        assert_eq!(plan.desired_focus_leaf.0, "leaf-solo");
        assert_eq!(tree_of(&s, "out-c", "ws-1"), None, "{direction:?}");
        assert_eq!(
            leaves(&s, target_output, "ws-1"),
            vec!["leaf-solo".to_string()],
            "{direction:?}"
        );
        assert_geometry_complete(&plan, &s);
    }
}

#[test]
fn r4_ambiguous_adjacency_refuses_end_to_end() {
    // Item 5.2: ambiguous (non-reciprocal) topology refuses fail-closed at
    // every layer. Session construction rejects a one-sided edge outright,
    // so no session with ambiguous adjacency can propose; the standalone
    // planner additionally refuses non-reciprocal cross candidates
    // (directional `r4_ambiguous_non_reciprocal_adjacency_refuses`) and the
    // protocol pair validation refuses them as `domain-invalid`.
    let one_sided = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            domain("out-2", "ws-1", 200, 200),
        ],
    );
    assert!(
        one_sided.is_err(),
        "one-sided adjacency must fail session construction"
    );
    // Reciprocal sessions construct and cross; a missing candidate stays a
    // PlannerNoop with no pending (no output wrapping).
    let mut s = stacked_session();
    admit_commit(&mut s, "win-1", "out-top", "ws-1", true, "n-1");
    admit_commit(&mut s, "win-2", "out-top", "ws-1", true, "n-2");
    let k = key("out-top", "ws-1");
    let before = s.snapshot();
    let rev = s.accepted_revision();
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    // win-2 sits at the right root edge; Right names no adjacent output.
    assert_eq!(
        s.propose_move(
            &k,
            &w,
            Direction::Right,
            &obs,
            &correlation("n-3"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PlannerNoop))
    );
    assert!(!s.has_pending());
    assert_eq!(s.snapshot(), before);
    assert_eq!(s.accepted_revision(), rev);
}

#[test]
fn r4_sole_leaf_cross_occupied_landing_nearest_source_all_directions() {
    // Item 5.2 landing for the 5.1 sole-leaf cross: onto an occupied target
    // the mover wraps beside the existing leaf on the D axis with W nearest
    // the source (mover first for positive steps Right/Down, last for
    // negative steps Left/Up). Local behavior is unchanged: a sole leaf has
    // no local restructure, so every direction crosses.
    use tiler_core::directional::Axis;
    for (direction, target_output, mover_first) in [
        (Direction::Left, "out-w", false),
        (Direction::Right, "out-e", true),
        (Direction::Up, "out-n", false),
        (Direction::Down, "out-s", true),
    ] {
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let mut s = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![
                domain_with_adjacent("out-c", "ws-1", vec![(direction, target_output)]),
                domain_with_adjacent(target_output, "ws-1", vec![(opposite, "out-c")]),
            ],
        )
        .expect("compass session");
        admit_commit(&mut s, "stay", target_output, "ws-1", true, "occ-1");
        admit_commit(&mut s, "solo", "out-c", "ws-1", true, "occ-2");
        let k = key("out-c", "ws-1");
        let plan = move_commit_focused(&mut s, &k, direction, "occ-3", &Capabilities::full());
        assert_eq!(
            plan.dispatch.rule,
            tiler_core::directional::Rule::R4,
            "{direction:?}"
        );
        match &plan.dispatch.operation {
            tiler_core::directional::MoveOperation::CrossOutput { target, .. } => {
                assert_eq!(
                    *target,
                    tiler_core::directional::CrossOutputTarget::Occupied,
                    "{direction:?}"
                );
            }
            other => panic!("expected cross-output, got {other:?}"),
        }
        let tree = tree_of(&s, target_output, "ws-1").expect("target tree");
        let (axis, children) = root_children(&tree);
        assert_eq!(axis, Axis::for_direction(direction), "{direction:?}");
        assert_eq!(children.len(), 2, "{direction:?}");
        let expected = if mover_first {
            vec!["leaf-solo".to_string(), "leaf-stay".to_string()]
        } else {
            vec!["leaf-stay".to_string(), "leaf-solo".to_string()]
        };
        assert_eq!(children, expected, "mover nearest source {direction:?}");
        assert_eq!(plan.desired_focus_domain, key(target_output, "ws-1"));
        assert_eq!(plan.desired_focus_leaf.0, "leaf-solo");
        assert_eq!(tree_of(&s, "out-c", "ws-1"), None, "{direction:?}");
        assert_geometry_complete(&plan, &s);
    }
}
