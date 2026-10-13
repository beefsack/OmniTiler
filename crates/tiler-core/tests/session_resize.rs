#![forbid(unsafe_code)]
//! Session keyboard split-share resize integration (portable, headless).
//!
//! Drives the real `session::Session` resize API (`propose_resize`,
//! `verify_resize`) through propose/acknowledge/verify cycles. No live
//! compositor state. Covers horizontal/vertical both directions, nested
//! ancestor resolution, N-ary pair-only effects, edge refusal,
//! positivity/normalization/clamp, focus retention, capability/stale/pending
//! refusals, exact acknowledgement/verification and cross-kind mismatch
//! divergence, deterministic replay, and a bounded property-style matrix.

use std::collections::BTreeMap;
use tiler_core::contract::{
    AckOutcome, AdapterAck, DivergenceKind, Observation, ResizeCapabilities, ResizePostObservation,
};
use tiler_core::directional::{Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand,
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
fn domain(output: &str, workspace: &str, w: i32, h: i32, gap: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap,
        adjacent: BTreeMap::new(),
    }
}
fn single_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 800, 600, 0)],
    )
    .expect("session")
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
fn complete_obs(
    session: &Session,
    extra: Vec<ObservedWindow>,
) -> tiler_core::session::SessionObservation {
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
    tiler_core::session::SessionObservation {
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
fn admit_commit(session: &mut Session, window: &str, horiz: bool, corr: &str) {
    let cmd = SessionCommand::Admit {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: placement(horiz),
        suppress_fixed_float: false,
    };
    let obs = complete_obs(session, vec![tiled(window, "out-1", "ws-1")]);
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &cmd,
            &obs,
            &correlation(corr),
            &tiler_core::contract::LifecycleCapabilities::full(),
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
}
fn focused_window(session: &Session, domain: &DomainKey) -> WindowId {
    let (d, l) = session.focus();
    let d = d.expect("domain");
    let l = l.expect("leaf");
    assert_eq!(&d, domain);
    session
        .snapshot()
        .windows
        .iter()
        .find(|w| w.leaf == l && w.output == domain.output && w.workspace == domain.workspace)
        .expect("link")
        .window
        .clone()
}
fn focus_commit(
    session: &mut Session,
    domain: &DomainKey,
    window: &WindowId,
    direction: Direction,
    corr: &str,
) {
    let obs = complete_obs(session, vec![]);
    let base = session.accepted_revision();
    let plan = session
        .propose_focus(
            domain,
            window,
            direction,
            &obs,
            &correlation(corr),
            &tiler_core::contract::FocusCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("focus {direction:?}: {e:?}"));
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
        .verify_focus(&tiler_core::contract::FocusPostObservation::new(
            Observation::new(owner(), generation(), base, 950 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify focus");
}
fn move_commit_focused(
    session: &mut Session,
    domain: &DomainKey,
    direction: Direction,
    corr: &str,
) {
    let w = focused_window(session, domain);
    let obs = complete_obs(session, vec![]);
    let base = session.accepted_revision();
    let plan = session
        .propose_move(
            domain,
            &w,
            direction,
            &obs,
            &correlation(corr),
            &tiler_core::directional::Capabilities::full(),
        )
        .unwrap_or_else(|e| panic!("move {direction:?}: {e:?}"));
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
        .verify_move(&tiler_core::contract::PostObservation::new(
            Observation::new(owner(), generation(), base, 300 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("move commit");
}
fn focus_commit_step(session: &mut Session, domain: &DomainKey, direction: Direction, corr: &str) {
    let w = focused_window(session, domain);
    focus_commit(session, domain, &w, direction, corr);
}
fn resize_commit(
    session: &mut Session,
    domain: &DomainKey,
    window: &WindowId,
    direction: Direction,
    corr: &str,
) -> tiler_core::session::SessionResizePlan {
    let obs = complete_obs(session, vec![]);
    let base = session.accepted_revision();
    let plan = session
        .propose_resize(
            domain,
            window,
            direction,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation(corr),
            &ResizeCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("resize {direction:?}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = ResizePostObservation::new(
        Observation::new(owner(), generation(), base, 960 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    let commit = session.verify_resize(&post).expect("verify resize");
    assert_eq!(commit.revision, base + 1);
    plan
}
fn root_shares(session: &Session) -> Vec<u64> {
    match session.snapshot().domains[0].tree.clone().expect("tree") {
        Node::Group { shares, .. } => shares,
        Node::Leaf { .. } => vec![],
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
fn assert_resize_geometry(plan: &tiler_core::session::SessionResizePlan) {
    assert!(!plan.desired_geometry.is_empty());
    for g in &plan.desired_geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0, "positive geometry");
        assert!(!g.window.0.is_empty() && !g.leaf.0.is_empty());
        assert_eq!(
            (&g.output, &g.workspace),
            (
                &plan.desired_focus_domain.output,
                &plan.desired_focus_domain.workspace
            )
        );
    }
    let view = plan
        .desired_snapshot
        .domains
        .iter()
        .find(|d| {
            d.output == plan.desired_focus_domain.output
                && d.workspace == plan.desired_focus_domain.workspace
        })
        .expect("domain");
    let mut expected = vec![];
    if let Some(tree) = &view.tree {
        collect_leaves(tree, &mut expected);
    }
    let mut got: Vec<String> = plan
        .desired_geometry
        .iter()
        .map(|g| g.leaf.0.clone())
        .collect();
    expected.sort();
    got.sort();
    assert_eq!(expected, got, "geometry covers affected domain");
    for i in 0..plan.desired_geometry.len() {
        for j in (i + 1)..plan.desired_geometry.len() {
            let (a, b) = (plan.desired_geometry[i].rect, plan.desired_geometry[j].rect);
            assert!(
                !(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h),
                "overlap"
            );
        }
    }
    let rendered = format!("{:?}", plan.dispatch.operation);
    assert!(!rendered.contains("kwin"));
}

#[test]
fn horizontal_both_directions() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    // Focus is win-2 (right). Edge Left with Outwards grows win-2: [1,1] -> [387,411] (12px).
    let w2 = focused_window(&s, &k);
    assert_eq!(w2.0, "win-2");
    let plan = resize_commit(&mut s, &k, &w2, Direction::Left, "r-1");
    assert_eq!(plan.resize_plan.operation.old_shares, vec![1, 1]);
    assert_eq!(plan.resize_plan.operation.new_shares, vec![387, 411]);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-2");
    assert_resize_geometry(&plan);
    // Now focus win-1 via focus Left, then resize Right grows win-1.
    focus_commit(
        &mut s,
        &k,
        &WindowId("win-2".to_owned()),
        Direction::Left,
        "f-1",
    );
    let w1 = focused_window(&s, &k);
    assert_eq!(w1.0, "win-1");
    // Current shares [387,411]; Right with Outwards grows win-1 back toward equal.
    let plan2 = resize_commit(&mut s, &k, &w1, Direction::Right, "r-2");
    assert_eq!(plan2.resize_plan.operation.old_shares, vec![387, 411]);
    assert_eq!(plan2.resize_plan.operation.new_shares, vec![399, 399]);
    assert_resize_geometry(&plan2);
}

#[test]
fn vertical_both_directions() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", false, "c-1");
    admit_commit(&mut s, "win-2", false, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let plan = resize_commit(&mut s, &k, &w2, Direction::Up, "r-1");
    assert_eq!(plan.resize_plan.operation.old_shares, vec![1, 1]);
    assert_eq!(plan.resize_plan.operation.new_shares, vec![287, 311]);
    assert_resize_geometry(&plan);
    focus_commit(
        &mut s,
        &k,
        &WindowId("win-2".to_owned()),
        Direction::Up,
        "f-1",
    );
    let w1 = focused_window(&s, &k);
    let plan2 = resize_commit(&mut s, &k, &w1, Direction::Down, "r-2");
    assert_eq!(plan2.resize_plan.operation.new_shares, vec![299, 299]);
    assert_resize_geometry(&plan2);
}

#[test]
fn nested_ancestor_resolution_outward() {
    // H[win-1, V[win-2, win-3]] with focus win-2. Direction Left skips inner
    // vertical group and resizes the outer horizontal group (V grows left).
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    admit_commit(&mut s, "win-3", false, "c-3");
    let k = key("out-1", "ws-1");
    // Focus win-3 currently; move focus to win-2 via Up (inner vertical).
    focus_commit(
        &mut s,
        &k,
        &WindowId("win-3".to_owned()),
        Direction::Up,
        "f-1",
    );
    let w = focused_window(&s, &k);
    assert_eq!(w.0, "win-2");
    let plan = resize_commit(&mut s, &k, &w, Direction::Left, "r-1");
    // Outer group [win-1, inner] [1,1] -> [389,409]; inner shares unchanged.
    assert_eq!(plan.resize_plan.operation.old_shares, vec![1, 1]);
    assert_eq!(plan.resize_plan.operation.new_shares, vec![387, 411]);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-2");
    let snap = s.snapshot();
    let root = snap.domains[0].tree.clone().expect("tree");
    match &root {
        Node::Group {
            children,
            shares,
            axis,
            ..
        } => {
            assert_eq!(*axis, tiler_core::directional::Axis::Horizontal);
            assert_eq!(*shares, vec![387, 411]);
            assert_eq!(children.len(), 2);
            match &children[1] {
                Node::Group { shares, .. } => assert_eq!(*shares, vec![1, 1], "inner untouched"),
                other => panic!("inner {other:?}"),
            }
        }
        other => panic!("root {other:?}"),
    }
    assert_resize_geometry(&plan);
}

#[test]
fn nary_pair_only_with_unaffected_subtree_and_order() {
    // Four-wide needs COSMIC-scale pairs: 1600px so the 400px children form
    // an 800px pair above the 720px pair minimum.
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 1600, 600, 0)],
    )
    .expect("session");
    for (i, c) in ["c-1", "c-2", "c-3", "c-4"].iter().enumerate() {
        admit_commit(&mut s, &format!("win-{}", i + 1), true, c);
    }
    let k = key("out-1", "ws-1");
    // Automatic admission always binary-wraps, so repeated admissions yield
    // nested binary groups. Flatten to the intended ordered 4-child N-ary
    // topology via public movement/focus commits (N-ary remains for
    // movement representation only), keeping exact window/focus links.
    move_commit_focused(&mut s, &k, Direction::Right, "m-1");
    focus_commit_step(&mut s, &k, Direction::Left, "f-1");
    focus_commit_step(&mut s, &k, Direction::Left, "f-2");
    move_commit_focused(&mut s, &k, Direction::Left, "m-2");
    focus_commit_step(&mut s, &k, Direction::Right, "f-3");
    move_commit_focused(&mut s, &k, Direction::Left, "m-3");
    focus_commit_step(&mut s, &k, Direction::Right, "f-4");
    // H[1,1,1,1] focus win-4 (last). Resize Left touches pair [win-3,win-4].
    let w4 = focused_window(&s, &k);
    assert_eq!(w4.0, "win-4");
    let before_leaves: Vec<String> = {
        let mut out = vec![];
        collect_leaves(
            &s.snapshot().domains[0].tree.clone().expect("tree"),
            &mut out,
        );
        out
    };
    let plan = resize_commit(&mut s, &k, &w4, Direction::Left, "r-1");
    // Only the adjacent pair redistributes (plus exact ratio-preserving
    // whole-group normalization for pixel precision).
    assert_eq!(plan.resize_plan.operation.old_shares, vec![1, 1, 1, 1]);
    assert_eq!(
        plan.resize_plan.operation.new_shares,
        vec![399, 399, 387, 411]
    );
    // Order unchanged.
    let mut after = vec![];
    collect_leaves(
        &s.snapshot().domains[0].tree.clone().expect("tree"),
        &mut after,
    );
    assert_eq!(before_leaves, after);
    assert_eq!(root_shares(&s), vec![399, 399, 387, 411]);
    assert_resize_geometry(&plan);
}

#[test]
fn edge_refusals_are_unchanged() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    let k = key("out-1", "ws-1");
    let w1 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    // Single root leaf has no boundary in any direction.
    for dir in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        assert_eq!(
            s.propose_resize(
                &k,
                &w1,
                dir,
                tiler_core::contract::ResizeMode::Outwards,
                0,
                &obs,
                &correlation("e-1"),
                &ResizeCapabilities::full()
            ),
            Err(ProposeError::Refused(RefusalKind::Unchanged))
        );
        assert!(!s.has_pending());
    }
    admit_commit(&mut s, "win-2", true, "c-2");
    // Focus win-2 at right edge: Right has no candidate.
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Right,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("e-2"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!s.has_pending());
}

#[test]
fn normalization_and_clamp_to_exhaustion() {
    // COSMIC policy checks through the versioned seam (no 1/16 project step).
    use tiler_core::cosmic_v1;
    use tiler_core::directional::Axis;
    assert_eq!(cosmic_v1::keyboard_step_px(0), 12);
    assert_eq!(cosmic_v1::keyboard_step_px(1), 14);
    assert_eq!(cosmic_v1::keyboard_step_px(4), 20);
    assert!(!cosmic_v1::pair_admits_resize(719, 600, Axis::Horizontal));
    assert!(cosmic_v1::pair_admits_resize(720, 600, Axis::Horizontal));
    assert!(!cosmic_v1::pair_admits_resize(800, 479, Axis::Vertical));
    assert!(cosmic_v1::pair_admits_resize(800, 480, Axis::Vertical));
    assert_eq!(
        cosmic_v1::clamp_pair_split(800, 10, Axis::Horizontal),
        Some((360, 440))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(800, 790, Axis::Horizontal),
        Some((440, 360))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(719, 360, Axis::Horizontal),
        None
    );
    // Session loop: repeatedly grow win-2 left until donor exhausts to Unchanged.
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let mut corr_n = 0;
    loop {
        let w = focused_window(&s, &k);
        assert_eq!(w.0, "win-2");
        let obs = complete_obs(&s, vec![]);
        corr_n += 1;
        let corr = format!("clamp-{corr_n}");
        match s.propose_resize(
            &k,
            &w,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation(&corr),
            &ResizeCapabilities::full(),
        ) {
            Ok(plan) => {
                for share in plan.resize_plan.operation.new_shares.iter() {
                    assert!(*share > 0);
                }
                let base = s.accepted_revision();
                s.acknowledge(&AdapterAck::new(
                    correlation(&corr),
                    owner(),
                    generation(),
                    base,
                    AckOutcome::Accepted,
                ))
                .expect("ack");
                s.verify_resize(&ResizePostObservation::new(
                    Observation::new(owner(), generation(), base, 500 + base),
                    correlation(&corr),
                    true,
                    plan.dispatch.preconditions.clone(),
                    plan.dispatch.operation.clone(),
                ))
                .expect("verify");
            }
            Err(ProposeError::Refused(RefusalKind::Unchanged)) => break,
            Err(other) => panic!("unexpected {other:?}"),
        }
        if corr_n > 30 {
            panic!("donor never exhausted");
        }
    }
    // Donor clamped at the COSMIC child minimum: the left child keeps 360px
    // and further Left proposals refuse as Unchanged.
    assert!(corr_n > 1, "at least one step must plan");
    let shares = root_shares(&s);
    assert!(shares[0] < shares[1], "left child shrank: {shares:?}");
    assert!(!s.has_pending());
}

#[test]
fn focus_retained_and_plan_carries_semantics() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let before_focus = s.focus();
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("sem-1"),
            &ResizeCapabilities::full(),
        )
        .expect("plan");
    assert_eq!(
        (
            Some(plan.desired_focus_domain.clone()),
            Some(plan.desired_focus_leaf.clone())
        ),
        before_focus
    );
    assert_eq!(plan.dispatch.base_revision, s.accepted_revision());
    assert_eq!(
        plan.dispatch.required_capability,
        tiler_core::contract::ResizeCapability::KeyboardResize
    );
    assert!(
        plan.dispatch
            .preconditions
            .contains(&tiler_core::contract::ResizePrecondition::AdapterMustVerifyPostconditions)
    );
    assert_eq!(
        plan.resize_plan.operation.target_group,
        plan.dispatch.operation.target_group
    );
    assert_eq!(plan.resize_plan.operation.old_shares, vec![1, 1]);
    assert_eq!(plan.resize_plan.operation.new_shares, vec![387, 411]);
    assert_eq!(plan.resize_plan.intent.direction, Direction::Left);
    assert_eq!(plan.resize_plan.intent.focused_window, w2);
    assert_resize_geometry(&plan);
    // Commit retains focus exactly.
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("sem-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_resize(&ResizePostObservation::new(
        Observation::new(owner(), generation(), base, 777),
        correlation("sem-1"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify");
    assert_eq!(s.focus(), before_focus);
}

#[test]
fn refusal_matrix_unknown_mismatch_capability_stale_pending() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    // Unknown domain.
    let bad_domain = key("out-9", "ws-1");
    assert_eq!(
        s.propose_resize(
            &bad_domain,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("x-1"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::UnknownDomain))
    );
    // Unknown window.
    assert_eq!(
        s.propose_resize(
            &k,
            &WindowId("win-9".to_owned()),
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("x-2"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::UnknownWindow))
    );
    // Focus mismatch: supply non-focused win-1.
    assert_eq!(
        s.propose_resize(
            &k,
            &WindowId("win-1".to_owned()),
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("x-3"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::FocusMismatch))
    );
    // Unsupported capability.
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("x-4"),
            &ResizeCapabilities::none()
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    assert!(!s.has_pending());
    // Stale revision diverges.
    let mut stale = obs.clone();
    stale.observation.revision += 1;
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &stale,
            &correlation("x-5"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
    );
    // Fresh session for pending test (stale diverged terminally above).
    let mut p = single_session();
    admit_commit(&mut p, "win-1", true, "c-1");
    admit_commit(&mut p, "win-2", true, "c-2");
    let pk = key("out-1", "ws-1");
    let pw = focused_window(&p, &pk);
    let pobs = complete_obs(&p, vec![]);
    p.propose_resize(
        &pk,
        &pw,
        Direction::Left,
        tiler_core::contract::ResizeMode::Outwards,
        0,
        &pobs,
        &correlation("pend-1"),
        &ResizeCapabilities::full(),
    )
    .expect("pending");
    assert!(p.has_pending());
    assert_eq!(
        p.propose_resize(
            &pk,
            &pw,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &pobs,
            &correlation("pend-2"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::PendingExists)
    );
}

#[test]
fn exact_ack_verify_and_cross_kind_mismatch_diverges() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let base = s.accepted_revision();
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("v-1"),
            &ResizeCapabilities::full(),
        )
        .expect("plan");
    // Wrong preconditions diverge.
    s.acknowledge(&AdapterAck::new(
        correlation("v-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    let mut bad_pre = plan.dispatch.preconditions.clone();
    bad_pre.pop();
    assert_eq!(
        s.verify_resize(&ResizePostObservation::new(
            Observation::new(owner(), generation(), base, 1),
            correlation("v-1"),
            true,
            bad_pre,
            plan.dispatch.operation.clone(),
        )),
        Err(tiler_core::reconcile::VerifyError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
}

#[test]
fn cross_kind_verify_diverges() {
    // Three-wide needs COSMIC-scale pairs (1600px) for a pending resize.
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 1600, 600, 0)],
    )
    .expect("session");
    // Admit a third window so a focus move exists, then hold a resize pending
    // and verify with a focus post-observation.
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    admit_commit(&mut s, "win-3", true, "c-3");
    let k = key("out-1", "ws-1");
    let w3 = focused_window(&s, &k);
    // Focus win-2 first so resize has a middle pair.
    focus_commit(&mut s, &k, &w3, Direction::Left, "f-1");
    let w2 = focused_window(&s, &k);
    assert_eq!(w2.0, "win-2");
    let obs = complete_obs(&s, vec![]);
    let base = s.accepted_revision();
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Right,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("x-1"),
            &ResizeCapabilities::full(),
        )
        .expect("resize");
    s.acknowledge(&AdapterAck::new(
        correlation("x-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    // Cross-kind: verify_resize pending with a focus post-observation diverges.
    let focus_op = tiler_core::contract::FocusOperation {
        domain_output: k.output.clone(),
        domain_workspace: k.workspace.clone(),
        from_leaf: Some(NodeId("leaf-win-2".to_owned())),
        to_leaf: NodeId("leaf-win-3".to_owned()),
        from_window: WindowId("win-2".to_owned()),
        to_window: WindowId("win-3".to_owned()),
        direction: Direction::Right,
        route: vec![NodeId("leaf-win-3".to_owned())],
        cross_source_output: None,
        cross_source_workspace: None,
    };
    let cross = tiler_core::contract::FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 2),
        correlation("x-1"),
        true,
        focus_op.preconditions(),
        focus_op,
    );
    assert_eq!(
        s.verify_focus(&cross),
        Err(tiler_core::reconcile::VerifyError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    let _ = plan;
}

#[test]
fn deterministic_replay() {
    fn build() -> (Session, DomainKey, WindowId) {
        let mut s = single_session();
        admit_commit(&mut s, "win-1", true, "c-1");
        admit_commit(&mut s, "win-2", true, "c-2");
        let k = key("out-1", "ws-1");
        let w = focused_window(&s, &k);
        (s, k, w)
    }
    let (mut a, ka, wa) = build();
    let (mut b, kb, wb) = build();
    let obsa = complete_obs(&a, vec![]);
    let obsb = complete_obs(&b, vec![]);
    let pa = a
        .propose_resize(
            &ka,
            &wa,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obsa,
            &correlation("rep-1"),
            &ResizeCapabilities::full(),
        )
        .expect("a");
    let pb = b
        .propose_resize(
            &kb,
            &wb,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obsb,
            &correlation("rep-1"),
            &ResizeCapabilities::full(),
        )
        .expect("b");
    assert_eq!(pa, pb);
    assert_eq!(pa.desired_geometry, pb.desired_geometry);
    // Commit both identically.
    for (s, p) in [(&mut a, &pa), (&mut b, &pb)] {
        let base = s.accepted_revision();
        s.acknowledge(&AdapterAck::new(
            correlation("rep-1"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
        s.verify_resize(&ResizePostObservation::new(
            Observation::new(owner(), generation(), base, 313),
            correlation("rep-1"),
            true,
            p.dispatch.preconditions.clone(),
            p.dispatch.operation.clone(),
        ))
        .expect("verify");
    }
    assert_eq!(a.snapshot(), b.snapshot());
}

#[test]
fn bounded_property_matrix() {
    // Varied shares (via repeated resizes), COSMIC-scale work areas, gaps,
    // directions. Checks: positive dimensions, span/gap conservation, share
    // validity, pair-sum conservation. Sub-minimum pairs refuse as PairBelowMinimum.
    let bounds_cases: Vec<(i32, i32, i32)> = vec![
        (800, 600, 0),
        (1440, 900, 2),
        (1600, 600, 4),
        (800, 900, 8),
        // Below the COSMIC pair minima: every proposal refuses as PairBelowMinimum.
        (200, 200, 0),
    ];
    let dir_cases: Vec<(bool, Vec<Direction>)> = vec![
        (true, vec![Direction::Left, Direction::Right]),
        (false, vec![Direction::Up, Direction::Down]),
    ];
    for (w, h, gap) in bounds_cases.clone() {
        for (horiz, dirs) in dir_cases.clone() {
            let mut s = Session::new(
                owner(),
                generation(),
                0,
                7,
                vec![domain("out-1", "ws-1", w, h, gap)],
            )
            .expect("session");
            admit_commit(&mut s, "win-1", horiz, "c-1");
            admit_commit(&mut s, "win-2", horiz, "c-2");
            admit_commit(&mut s, "win-3", horiz, "c-3");
            let k = key("out-1", "ws-1");
            for dir in dirs {
                // Focus middle window win-2 for a middle pair test when possible.
                // Current focus is win-3; move to win-2.
                let cur = focused_window(&s, &k);
                if cur.0 == "win-3" {
                    // Move focus one step toward win-2 (best effort, single proposal).
                    let back = match dir {
                        Direction::Left => Direction::Left,
                        Direction::Right => Direction::Left,
                        Direction::Up => Direction::Up,
                        Direction::Down => Direction::Up,
                    };
                    let obs = complete_obs(&s, vec![]);
                    let fcorr = format!("m-f-{w}-{h}-{gap}-{dir:?}");
                    if let Ok(fplan) = s.propose_focus(
                        &k,
                        &cur,
                        back,
                        &obs,
                        &correlation(&fcorr),
                        &tiler_core::contract::FocusCapabilities::full(),
                    ) {
                        let base = s.accepted_revision();
                        s.acknowledge(&AdapterAck::new(
                            correlation(&fcorr),
                            owner(),
                            generation(),
                            base,
                            AckOutcome::Accepted,
                        ))
                        .expect("ack");
                        s.verify_focus(&tiler_core::contract::FocusPostObservation::new(
                            Observation::new(owner(), generation(), base, 700 + base),
                            correlation(&fcorr),
                            true,
                            fplan.dispatch.preconditions.clone(),
                            fplan.dispatch.operation.clone(),
                        ))
                        .expect("focus");
                    }
                }
                let fw = focused_window(&s, &k);
                let obs = complete_obs(&s, vec![]);
                let corr = format!("m-{w}-{h}-{gap}-{dir:?}");
                let plan = match s.propose_resize(
                    &k,
                    &fw,
                    dir,
                    tiler_core::contract::ResizeMode::Outwards,
                    0,
                    &obs,
                    &correlation(&corr),
                    &ResizeCapabilities::full(),
                ) {
                    Ok(p) => p,
                    Err(ProposeError::Refused(RefusalKind::Unchanged)) => continue,
                    Err(ProposeError::Refused(RefusalKind::PairBelowMinimum)) => continue,
                    Err(e) => panic!("matrix {w}x{h} gap {gap} {dir:?}: {e:?}"),
                };
                // No zero shares, totals valid.
                assert!(!plan.resize_plan.operation.new_shares.contains(&0));
                let total: u64 = plan.resize_plan.operation.new_shares.iter().sum();
                assert!(total > 0);
                // Geometry positive, contained, non-overlap, span conserved.
                for g in &plan.desired_geometry {
                    assert!(g.rect.w > 0 && g.rect.h > 0, "no zero dims {w}x{h} {dir:?}");
                }
                let is_horiz = matches!(
                    plan.resize_plan.operation.direction,
                    Direction::Left | Direction::Right
                );
                if is_horiz {
                    let min_x = plan
                        .desired_geometry
                        .iter()
                        .map(|g| g.rect.x)
                        .min()
                        .unwrap();
                    let max_e = plan
                        .desired_geometry
                        .iter()
                        .map(|g| g.rect.x + g.rect.w)
                        .max()
                        .unwrap();
                    let dom = &s.domains()[0];
                    assert_eq!(min_x, dom.bounds.x);
                    assert_eq!(max_e, dom.bounds.x + dom.bounds.w);
                    let mut xs: Vec<_> = plan
                        .desired_geometry
                        .iter()
                        .map(|g| (g.rect.x, g.rect.w))
                        .collect();
                    xs.sort();
                    for pair in xs.windows(2) {
                        assert_eq!(pair[1].0, pair[0].0 + pair[0].1 + dom.gap, "gap conserved");
                    }
                } else {
                    let min_y = plan
                        .desired_geometry
                        .iter()
                        .map(|g| g.rect.y)
                        .min()
                        .unwrap();
                    let max_e = plan
                        .desired_geometry
                        .iter()
                        .map(|g| g.rect.y + g.rect.h)
                        .max()
                        .unwrap();
                    let dom = &s.domains()[0];
                    assert_eq!(min_y, dom.bounds.y);
                    assert_eq!(max_e, dom.bounds.y + dom.bounds.h);
                }
                // COSMIC pixel check: the adjacent pair total is conserved
                // exactly (after whole-group ratio-preserving normalization)
                // and every share stays positive.
                let old = plan.resize_plan.operation.old_shares.clone();
                let new = plan.resize_plan.operation.new_shares.clone();
                let fi = plan.resize_plan.operation.focused_index;
                let ni = plan.resize_plan.operation.neighbor_index;
                let old_pair = old[fi] + old[ni];
                let new_pair = new[fi] + new[ni];
                assert!(new_pair > 0 && old_pair > 0);
                // Whole-group scale factor witnessed off-pair (or pair ratio
                // for two-child groups) must be exact.
                let n = old.len();
                let has_non_pair = (0..n).any(|i| i != fi && i != ni);
                if has_non_pair {
                    let witness = (0..n).find(|i| *i != fi && *i != ni).expect("witness");
                    assert!(new[witness] % old[witness] == 0);
                    let scale = new[witness] / old[witness];
                    assert!(scale >= 1);
                    assert_eq!(new_pair, old_pair * scale);
                } else {
                    assert!(new_pair % old_pair == 0);
                }
                // Commit to keep session usable for next direction.
                let base = s.accepted_revision();
                s.acknowledge(&AdapterAck::new(
                    correlation(&corr),
                    owner(),
                    generation(),
                    base,
                    AckOutcome::Accepted,
                ))
                .expect("ack");
                s.verify_resize(&ResizePostObservation::new(
                    Observation::new(owner(), generation(), base, 800 + base),
                    correlation(&corr),
                    true,
                    plan.dispatch.preconditions.clone(),
                    plan.dispatch.operation.clone(),
                ))
                .expect("verify");
            }
        }
    }
}

#[test]
fn reversible_paired_directions_without_clamp() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    // Reach the pixel steady state: [1,1] -> Left from win-2 gives [389,409]
    // (400px halves, 12px step), then Right from win-1 gives proportional shares.
    let w2 = focused_window(&s, &k);
    resize_commit(&mut s, &k, &w2, Direction::Left, "fwd-0");
    focus_commit(
        &mut s,
        &k,
        &WindowId("win-2".to_owned()),
        Direction::Left,
        "f-0",
    );
    let w1 = focused_window(&s, &k);
    assert_eq!(w1.0, "win-1");
    resize_commit(&mut s, &k, &w1, Direction::Right, "fwd-00");
    assert_eq!(root_shares(&s), vec![399, 399]);
    // Now reversible without clamp: Left from win-2 -> [389,409], then Right
    // from win-1 inverts exactly back to [399,399].
    focus_commit(
        &mut s,
        &k,
        &WindowId("win-1".to_owned()),
        Direction::Right,
        "f-1",
    );
    let w2b = focused_window(&s, &k);
    assert_eq!(w2b.0, "win-2");
    let fwd = resize_commit(&mut s, &k, &w2b, Direction::Left, "fwd-1");
    assert_eq!(fwd.resize_plan.operation.old_shares, vec![399, 399]);
    assert_eq!(fwd.resize_plan.operation.new_shares, vec![387, 411]);
    focus_commit(
        &mut s,
        &k,
        &WindowId("win-2".to_owned()),
        Direction::Left,
        "f-2",
    );
    let w1b = focused_window(&s, &k);
    let back = resize_commit(&mut s, &k, &w1b, Direction::Right, "back-1");
    assert_eq!(back.resize_plan.operation.old_shares, vec![387, 411]);
    assert_eq!(back.resize_plan.operation.new_shares, vec![399, 399]);
    assert_eq!(root_shares(&s), vec![399, 399]);
}

#[test]
fn capability_missing_at_edge_is_unsupported_not_unchanged() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    let k = key("out-1", "ws-1");
    let w1 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    // Single leaf: no boundary. Without capability this must still classify
    // as UnsupportedCapability (capability checked before boundary planning).
    assert_eq!(
        s.propose_resize(
            &k,
            &w1,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("cap-edge-1"),
            &ResizeCapabilities::none()
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    assert!(!s.has_pending());
    assert!(!s.has_pending_desired());
    // With capability the same edge is Unchanged.
    assert_eq!(
        s.propose_resize(
            &k,
            &w1,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("cap-edge-2"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!s.has_pending());
}

#[test]
fn cosmic_fixed_minima_govern_without_separate_capability() {
    // COSMIC fixed 360/240 minima live under cosmic_v1 with normalized
    // projection supplying physical geometry; no separate native capability
    // exists. Missing keyboard-resize still refuses as unsupported.
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("cap-native-1"),
            &ResizeCapabilities::none()
        ),
        Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
    );
    assert!(!s.has_pending());
    assert!(!s.has_pending_desired());
    // Full capabilities plan on the same state under fixed minima.
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("cap-native-2"),
            &ResizeCapabilities::full(),
        )
        .expect("plan under fixed minima");
    assert_eq!(plan.resize_plan.operation.new_shares, vec![387, 411]);
}

#[test]
fn flagged_observation_refuses_fail_closed() {
    use tiler_core::session::{ExceptionBehavior, SessionCommand};
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    // Flagged (floating) observation for a tiled focused window does not
    // match known tiled bindings: fail-closed refusal with no pending.
    let mut obs = complete_obs(&s, vec![]);
    for entry in obs.windows.iter_mut() {
        if entry.window == w2 {
            entry.floating = true;
        }
    }
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("flag-1"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert!(!s.has_pending());
    assert!(!s.has_pending_desired());
    // Exception-tracked window ids are non-tiled: defer win-9 as floating,
    // then resizing it refuses as NotTiled.
    let mut t = single_session();
    admit_commit(&mut t, "win-1", true, "c-1");
    let defer = SessionCommand::Admit {
        window: WindowId("win-9".to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        exceptions: ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        exception_behavior: Some(ExceptionBehavior::Defer),
        placement_bounds: placement(true),
        suppress_fixed_float: false,
    };
    let tobs = complete_obs(
        &t,
        vec![ObservedWindow {
            window: WindowId("win-9".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        }],
    );
    let tbase = t.accepted_revision();
    let dplan = t
        .propose(
            &defer,
            &tobs,
            &correlation("flag-defer"),
            &tiler_core::contract::LifecycleCapabilities::full(),
        )
        .expect("defer");
    t.acknowledge(&AdapterAck::new(
        correlation("flag-defer"),
        owner(),
        generation(),
        tbase,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    t.verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), tbase, 900 + tbase),
        correlation("flag-defer"),
        true,
        dplan.dispatch.preconditions.clone(),
        dplan.dispatch.operation.clone(),
    ))
    .expect("commit");
    let tk = key("out-1", "ws-1");
    let tkobs = complete_obs(&t, vec![]);
    assert_eq!(
        t.propose_resize(
            &tk,
            &WindowId("win-9".to_owned()),
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &tkobs,
            &correlation("flag-2"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::NotTiled))
    );
    assert!(!t.has_pending());
}

#[test]
fn partial_cross_domain_malformed_observations_refuse() {
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    // Partial: drop one window.
    let mut partial = complete_obs(&s, vec![]);
    partial.windows.pop();
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &partial,
            &correlation("obs-partial"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert!(!s.has_pending());
    // Cross-domain: retarget one entry to an unknown output.
    let mut cross = complete_obs(&s, vec![]);
    cross.windows[0].output = OutputId("out-9".to_owned());
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &cross,
            &correlation("obs-cross"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch))
    );
    assert!(!s.has_pending());
    // Malformed: empty window identity.
    let mut malformed = complete_obs(&s, vec![]);
    malformed.windows[0].window = WindowId(String::new());
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &malformed,
            &correlation("obs-malformed"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::MalformedInput))
    );
    assert!(!s.has_pending());
    // Malformed: duplicate window identity.
    let mut dup = complete_obs(&s, vec![]);
    dup.windows.push(dup.windows[0].clone());
    assert_eq!(
        s.propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &dup,
            &correlation("obs-dup"),
            &ResizeCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::MalformedInput))
    );
    assert!(!s.has_pending());
}

#[test]
fn overflow_shares_are_unrepresentable_malformed() {
    use tiler_core::directional::{Axis, Node as DNode};
    use tiler_core::directional::{NodeId as DNodeId, expected_resize_shares, plan_resize_step};
    // Valid topology with an unrepresentable x16 normalization.
    let tree = DNode::Group {
        id: DNodeId("root".to_owned()),
        axis: Axis::Horizontal,
        children: vec![
            DNode::Leaf {
                id: DNodeId("A".to_owned()),
            },
            DNode::Leaf {
                id: DNodeId("B".to_owned()),
            },
        ],
        shares: vec![u64::MAX - 100, 1],
    };
    assert_eq!(
        plan_resize_step(&tree, &DNodeId("A".to_owned()), Direction::Right),
        Err(tiler_core::directional::ResizePlanError::Malformed)
    );
    assert_eq!(expected_resize_shares(&[u64::MAX - 100, 1], 0, 1), None);
}

#[test]
fn vertical_gap_conservation() {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 800, 600, 4)],
    )
    .expect("session");
    admit_commit(&mut s, "win-1", false, "c-1");
    admit_commit(&mut s, "win-2", false, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Up,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("vgap-1"),
            &ResizeCapabilities::full(),
        )
        .expect("vertical plan");
    assert!(plan.resize_plan.operation.direction == Direction::Up);
    let dom = &s.domains()[0];
    let min_y = plan
        .desired_geometry
        .iter()
        .map(|g| g.rect.y)
        .min()
        .unwrap();
    let max_e = plan
        .desired_geometry
        .iter()
        .map(|g| g.rect.y + g.rect.h)
        .max()
        .unwrap();
    assert_eq!(min_y, dom.bounds.y);
    assert_eq!(max_e, dom.bounds.y + dom.bounds.h);
    let mut ys: Vec<(i32, i32)> = plan
        .desired_geometry
        .iter()
        .map(|g| (g.rect.y, g.rect.h))
        .collect();
    ys.sort();
    assert_eq!(ys.len(), 2);
    assert_eq!(
        ys[1].0,
        ys[0].0 + ys[0].1 + dom.gap,
        "vertical gap conserved"
    );
    for g in &plan.desired_geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
    }
}

#[test]
fn divisible_step_inverts_exactly_via_primitive() {
    // COSMIC pixel steady state on the vertical axis inverts exactly through
    // the session: Up from win-2 gives [287,311] (300px halves, 12px step),
    // then Down from win-1 returns [299,299], and the cycle repeats.
    let mut s = single_session();
    admit_commit(&mut s, "win-1", false, "c-1");
    admit_commit(&mut s, "win-2", false, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let fwd = resize_commit(&mut s, &k, &w2, Direction::Up, "v-fwd");
    assert_eq!(fwd.resize_plan.operation.old_shares, vec![1, 1]);
    assert_eq!(fwd.resize_plan.operation.new_shares, vec![287, 311]);
    focus_commit(&mut s, &k, &w2, Direction::Up, "v-f");
    let w1 = focused_window(&s, &k);
    let back = resize_commit(&mut s, &k, &w1, Direction::Down, "v-back");
    assert_eq!(back.resize_plan.operation.old_shares, vec![287, 311]);
    assert_eq!(back.resize_plan.operation.new_shares, vec![299, 299]);
    assert_eq!(root_shares(&s), vec![299, 299]);
    // A second cycle inverts through the same steady states.
    focus_commit(&mut s, &k, &w1, Direction::Down, "v-f2");
    let w2b = focused_window(&s, &k);
    let fwd2 = resize_commit(&mut s, &k, &w2b, Direction::Up, "v-fwd2");
    assert_eq!(fwd2.resize_plan.operation.new_shares, vec![287, 311]);
}

#[test]
fn pending_desired_clears_on_resize_terminal_divergence() {
    use tiler_core::contract::DivergenceKind;
    // Verify-path divergence clears staged desired state.
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let base = s.accepted_revision();
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("div-1"),
            &ResizeCapabilities::full(),
        )
        .expect("plan");
    assert!(s.has_pending());
    assert!(s.has_pending_desired());
    s.acknowledge(&AdapterAck::new(
        correlation("div-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    let mut bad_pre = plan.dispatch.preconditions.clone();
    bad_pre.pop();
    assert_eq!(
        s.verify_resize(&ResizePostObservation::new(
            Observation::new(owner(), generation(), base, 1),
            correlation("div-1"),
            true,
            bad_pre,
            plan.dispatch.operation.clone(),
        )),
        Err(tiler_core::reconcile::VerifyError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert!(!s.has_pending());
    assert!(!s.has_pending_desired());
    // Ack-path divergence clears staged desired state on a fresh session.
    let mut t = single_session();
    admit_commit(&mut t, "win-1", true, "c-1");
    admit_commit(&mut t, "win-2", true, "c-2");
    let tk = key("out-1", "ws-1");
    let tw = focused_window(&t, &tk);
    let tobs = complete_obs(&t, vec![]);
    let tbase = t.accepted_revision();
    t.propose_resize(
        &tk,
        &tw,
        Direction::Left,
        tiler_core::contract::ResizeMode::Outwards,
        0,
        &tobs,
        &correlation("div-2"),
        &ResizeCapabilities::full(),
    )
    .expect("plan");
    assert!(t.has_pending_desired());
    assert_eq!(
        t.acknowledge(&AdapterAck::new(
            correlation("wrong"),
            owner(),
            generation(),
            tbase,
            AckOutcome::Accepted,
        )),
        Err(tiler_core::reconcile::AckError::Diverged(
            DivergenceKind::CorrelationMismatch
        ))
    );
    assert!(!t.has_pending_desired());
}

#[test]
fn keyboard_inwards_shrinks_focused_outwards_grows() {
    use tiler_core::contract::ResizeMode;
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    assert_eq!(w2.0, "win-2");
    // Outwards (grow focused right via Left edge) moves left boundary left.
    let obs = complete_obs(&s, vec![]);
    let out_plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &obs,
            &correlation("inout-out"),
            &ResizeCapabilities::full(),
        )
        .expect("outwards plans");
    assert_eq!(out_plan.resize_plan.operation.new_shares, vec![387, 411]);
    assert_eq!(out_plan.resize_plan.intent.mode, ResizeMode::Outwards);
    // Inwards (shrink focused) moves the same edge the opposite way.
    let in_plan = {
        let mut tmp = single_session();
        admit_commit(&mut tmp, "win-1", true, "c-i1");
        admit_commit(&mut tmp, "win-2", true, "c-i2");
        let tk = key("out-1", "ws-1");
        let tw = focused_window(&tmp, &tk);
        let tobs = complete_obs(&tmp, vec![]);
        tmp.propose_resize(
            &tk,
            &tw,
            Direction::Left,
            ResizeMode::Inwards,
            0,
            &tobs,
            &correlation("inout-in"),
            &ResizeCapabilities::full(),
        )
        .expect("inwards plans")
    };
    // Inwards on the right child increases the left share (mirror of outwards).
    assert_eq!(in_plan.resize_plan.operation.old_shares, vec![1, 1]);
    assert_eq!(in_plan.resize_plan.intent.mode, ResizeMode::Inwards);
    let out_left = out_plan.resize_plan.operation.new_shares[0];
    let in_left = in_plan.resize_plan.operation.new_shares[0];
    assert!(
        out_left < 399 && in_left > 399,
        "out {out_left} in {in_left}"
    );
}

#[test]
fn pair_threshold_uses_direct_sum_not_union_with_gap() {
    // Direct pair sums gate the resize, never union including gap (resize.rs
    // 291-317, tiling 2582-2600). Width 727 gap 8 => sizes 359+360=719 refuse
    // (union 727 would admit if misused); wider admits. Height 487 gap 8 =>
    // 239+240=479 refuse; taller admits.
    fn session_with(w: i32, h: i32, gap: i32) -> Session {
        Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![domain("out-1", "ws-1", w, h, gap)],
        )
        .expect("session")
    }
    fn admit_wide(session: &mut Session, window: &str, corr: &str) {
        admit_commit(session, window, true, corr);
    }
    // Horizontal 719 refuse.
    let mut s = session_with(727, 600, 8);
    admit_wide(&mut s, "win-1", "g-1");
    admit_wide(&mut s, "win-2", "g-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_resize(
            &k,
            &w,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("gap-719"),
            &ResizeCapabilities::full(),
        ),
        Err(ProposeError::Refused(RefusalKind::PairBelowMinimum))
    );
    // Horizontal admit with gap (direct sum 792, union 800): plans.
    let mut s2 = session_with(800, 600, 8);
    admit_wide(&mut s2, "win-1", "g-1");
    admit_wide(&mut s2, "win-2", "g-2");
    let k2 = key("out-1", "ws-1");
    let w2 = focused_window(&s2, &k2);
    let obs2 = complete_obs(&s2, vec![]);
    assert!(
        s2.propose_resize(
            &k2,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs2,
            &correlation("gap-720"),
            &ResizeCapabilities::full(),
        )
        .is_ok()
    );
    // Vertical 479 refuse, 480 admit (gap 8).
    let mut v = session_with(800, 487, 8);
    admit_commit(&mut v, "win-1", false, "v-1");
    admit_commit(&mut v, "win-2", false, "v-2");
    let vk = key("out-1", "ws-1");
    let vw = focused_window(&v, &vk);
    let vobs = complete_obs(&v, vec![]);
    assert_eq!(
        v.propose_resize(
            &vk,
            &vw,
            Direction::Up,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &vobs,
            &correlation("gap-479"),
            &ResizeCapabilities::full(),
        ),
        Err(ProposeError::Refused(RefusalKind::PairBelowMinimum))
    );
    let mut v2 = session_with(800, 600, 8);
    admit_commit(&mut v2, "win-1", false, "v-1");
    admit_commit(&mut v2, "win-2", false, "v-2");
    let vk2 = key("out-1", "ws-1");
    let vw2 = focused_window(&v2, &vk2);
    let vobs2 = complete_obs(&v2, vec![]);
    assert!(
        v2.propose_resize(
            &vk2,
            &vw2,
            Direction::Up,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &vobs2,
            &correlation("gap-480"),
            &ResizeCapabilities::full(),
        )
        .is_ok()
    );
}

#[test]
fn keyboard_one_sided_clamp_and_dedicated_reconcile_path() {
    // One-sided source clamp differs from pointer two-sided when the grow
    // side sits below the child minimum: keyboard keeps grow at 232, pointer
    // would correct to 360/360.
    use tiler_core::cosmic_v1;
    use tiler_core::directional::Axis;
    assert_eq!(
        cosmic_v1::clamp_keyboard_shrink_pair(500, 220, 12, Axis::Horizontal),
        Some((488, 232))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(720, 488, Axis::Horizontal),
        Some((360, 360))
    );
    // Session keyboard carries mode/direction semantics and commits through
    // the dedicated propose_resize path (one pending + exact ack/verify).
    let mut s = single_session();
    admit_commit(&mut s, "win-1", true, "c-1");
    admit_commit(&mut s, "win-2", true, "c-2");
    let k = key("out-1", "ws-1");
    let w2 = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    let plan = s
        .propose_resize(
            &k,
            &w2,
            Direction::Left,
            tiler_core::contract::ResizeMode::Inwards,
            0,
            &obs,
            &correlation("kbd-1"),
            &ResizeCapabilities::full(),
        )
        .expect("keyboard inwards plans");
    assert_eq!(
        plan.resize_plan.intent.mode,
        tiler_core::contract::ResizeMode::Inwards
    );
    assert_eq!(
        plan.dispatch.operation.mode,
        tiler_core::contract::ResizeMode::Inwards
    );
    // Inwards on the right child shrinks focused: the focused desired
    // width is smaller than the accepted width (share integers scale for
    // pixel precision, so compare geometry, not raw shares).
    let accepted = complete_obs(&s, vec![]);
    let _ = accepted;
    let focused_leaf = plan.resize_plan.operation.focused_child.clone();
    let before_w = {
        // Project accepted tree via session snapshot geometry helper: use the
        // desired geometry of a no-op? Instead compare Inwards vs Outwards.
        let mut t = single_session();
        admit_commit(&mut t, "win-1", true, "c-1");
        admit_commit(&mut t, "win-2", true, "c-2");
        let tk = key("out-1", "ws-1");
        let tw = focused_window(&t, &tk);
        let tobs = complete_obs(&t, vec![]);
        let out = t
            .propose_resize(
                &tk,
                &tw,
                Direction::Left,
                tiler_core::contract::ResizeMode::Outwards,
                0,
                &tobs,
                &correlation("kbd-out"),
                &ResizeCapabilities::full(),
            )
            .expect("outwards plans");
        let inw_focused_w = plan
            .desired_geometry
            .iter()
            .find(|g| g.leaf == focused_leaf)
            .expect("inwards geometry")
            .rect
            .w;
        let out_focused_w = out
            .desired_geometry
            .iter()
            .find(|g| g.leaf == out.resize_plan.operation.focused_child)
            .expect("outwards geometry")
            .rect
            .w;
        assert!(
            inw_focused_w < out_focused_w,
            "inwards {inw_focused_w} must shrink vs outwards {out_focused_w}"
        );
        inw_focused_w
    };
    let _ = before_w;
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("kbd-1"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_resize(&ResizePostObservation::new(
        Observation::new(owner(), generation(), base, 910 + base),
        correlation("kbd-1"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify");
    // Reconciliation rejects an invalid keyboard fixed-share operation
    // (broken integer scaling) on the dedicated path.
    use tiler_core::contract::{ResizeIntent, ResizeOperation};
    use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::reconcile::Reconciler;
    let owner = OwnerId::parse("owner-1").expect("owner");
    let generation = GenerationId::parse("gen-1").expect("gen");
    let mut r = Reconciler::new(owner.clone(), generation.clone(), 0, 11).expect("reconciler");
    let intent = ResizeIntent {
        domain_output: OutputId("out-1".to_owned()),
        domain_workspace: WorkspaceId("ws-1".to_owned()),
        focused_leaf: NodeId("leaf-win-2".to_owned()),
        focused_window: WindowId("win-2".to_owned()),
        direction: Direction::Left,
        mode: tiler_core::contract::ResizeMode::Outwards,
    };
    let bad_op = ResizeOperation {
        domain_output: intent.domain_output.clone(),
        domain_workspace: intent.domain_workspace.clone(),
        focused_leaf: intent.focused_leaf.clone(),
        focused_window: intent.focused_window.clone(),
        direction: Direction::Left,
        mode: tiler_core::contract::ResizeMode::Outwards,
        target_group: NodeId("root".to_owned()),
        focused_child: NodeId("leaf-win-2".to_owned()),
        neighbor_child: NodeId("leaf-win-1".to_owned()),
        focused_index: 1,
        neighbor_index: 0,
        old_shares: vec![1, 1],
        new_shares: vec![14, 17],
    };
    let bad_plan = tiler_core::contract::ResizePlan::for_operation(intent, bad_op);
    let obs = tiler_core::contract::Observation::new(owner, generation, 0, 11);
    assert!(matches!(
        r.propose_resize(
            &bad_plan,
            &obs,
            &CorrelationId::parse("corr-1").expect("corr"),
            &ResizeCapabilities::full(),
        ),
        Err(tiler_core::reconcile::ProposeError::Diverged(_))
    ));
}

#[test]
fn pair_minimum_refuses_distinct_kind() {
    // Sub-minimum direct pair sums refuse as PairBelowMinimum with
    // thresholds 720/480 unchanged; other no-change stays Unchanged.
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 727, 600, 8)],
    )
    .expect("session");
    admit_commit(&mut s, "win-1", true, "t-1");
    admit_commit(&mut s, "win-2", true, "t-2");
    let k = key("out-1", "ws-1");
    let w = focused_window(&s, &k);
    let obs = complete_obs(&s, vec![]);
    assert_eq!(
        s.propose_resize(
            &k,
            &w,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &obs,
            &correlation("t-pair-min"),
            &ResizeCapabilities::full(),
        ),
        Err(ProposeError::Refused(RefusalKind::PairBelowMinimum))
    );
    assert!(!s.has_pending());
    assert_eq!(
        ProposeError::Refused(RefusalKind::PairBelowMinimum).kind(),
        "pair-below-minimum"
    );
    assert_eq!(
        ProposeError::Refused(RefusalKind::PairBelowMinimum).message(),
        "resize pair is below the minimum size"
    );
    // Other no-change is preserved: single leaf with no boundary stays Unchanged.
    let mut e = single_session();
    admit_commit(&mut e, "win-1", true, "t-e1");
    let ek = key("out-1", "ws-1");
    let ew = focused_window(&e, &ek);
    let eobs = complete_obs(&e, vec![]);
    assert_eq!(
        e.propose_resize(
            &ek,
            &ew,
            Direction::Left,
            tiler_core::contract::ResizeMode::Outwards,
            0,
            &eobs,
            &correlation("t-edge"),
            &ResizeCapabilities::full(),
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!e.has_pending());
}
