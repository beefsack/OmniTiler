#![forbid(unsafe_code)]
//! Cross-output directional focus/movement with distinct active workspaces.
//!
//! Offline Rust authority tests for the source's default Vertical layout
//! output axis only: `Meta+Left/Right` cross horizontally adjacent outputs
//! only after local focus/move is exhausted, targeting the adjacent output's
//! currently selected logical workspace (which may differ from the source
//! workspace). `Meta+Up/Down` retain local behavior. No workspace cycling,
//! wrapping, or shortcuts. Fake-test disclaimer: these assert the portable
//! planning/validation logic only, not actual output-switch acceptance.

use tiler_core::contract::{
    AckOutcome, AdapterAck, FocusCapabilities, FocusPostObservation, Observation, PostObservation,
};
use tiler_core::directional::{Axis, Capabilities, Direction, OutputId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionObservation,
};

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).unwrap_or_else(|| panic!("valid {value}"))
}
fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}
fn domain_ws(
    output: &str,
    workspace: &str,
    adjacent: Vec<(Direction, &str)>,
    gap: i32,
) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        },
        gap,
        adjacent: adjacent
            .into_iter()
            .map(|(d, t)| (d, OutputId(t.to_owned())))
            .collect(),
    }
}
fn two_output_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")], 4),
            domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")], 8),
        ],
    )
    .expect("two-output session")
}
fn complete_obs(session: &Session) -> SessionObservation {
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
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            session.accepted_revision(),
            500 + session.accepted_revision(),
        ),
        windows,
    }
}
fn admit(session: &mut Session, window: &str, output: &str, workspace: &str, corr: &str) {
    admit_with_placement(
        session,
        window,
        output,
        workspace,
        corr,
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
    );
}
fn admit_with_placement(
    session: &mut Session,
    window: &str,
    output: &str,
    workspace: &str,
    corr: &str,
    placement_bounds: Rect,
) {
    use tiler_core::contract::LifecycleCapabilities;
    let base = session.accepted_revision();
    let mut observed: Vec<ObservedWindow> = session
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
    observed.extend(session.exception_observed());
    observed.push(ObservedWindow {
        window: tiler_core::directional::WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    });
    observed.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let correlation = correlation(corr);
    let observation = SessionObservation {
        observation: Observation::new(owner(), generation(), base, base),
        windows: observed,
    };
    let plan = session
        .propose(
            &SessionCommand::Admit {
                window: tiler_core::directional::WindowId(window.to_owned()),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds,
                suppress_fixed_float: false,
            },
            &observation,
            &correlation,
            &LifecycleCapabilities::full(),
        )
        .expect("admit");
    session
        .acknowledge(&AdapterAck::new(
            correlation.clone(),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, base),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
}
fn focused_window(session: &Session, domain: &DomainKey) -> tiler_core::directional::WindowId {
    let (Some(focus_domain), Some(focus_leaf)) = session.focus() else {
        panic!("no focus")
    };
    assert_eq!(&focus_domain, domain);
    session
        .snapshot()
        .windows
        .iter()
        .find(|l| {
            l.leaf == focus_leaf && l.output == domain.output && l.workspace == domain.workspace
        })
        .expect("focused link")
        .window
        .clone()
}
fn window_in(
    session: &Session,
    domain: &DomainKey,
    window: &str,
) -> tiler_core::directional::WindowId {
    session
        .snapshot()
        .windows
        .iter()
        .find(|l| {
            l.window.0 == window && l.output == domain.output && l.workspace == domain.workspace
        })
        .expect("domain window")
        .window
        .clone()
}
fn move_commit(
    session: &mut Session,
    domain: &DomainKey,
    direction: Direction,
    corr: &str,
) -> tiler_core::session::SessionMovePlan {
    let window = focused_window(session, domain);
    let obs = complete_obs(session);
    let before_rev = session.accepted_revision();
    let plan = session
        .propose_move(
            domain,
            &window,
            direction,
            &obs,
            &correlation(corr),
            &Capabilities::full(),
        )
        .expect("move plans");
    assert_eq!(plan.dispatch.base_revision, before_rev);
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            before_rev,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = PostObservation::new(
        Observation::new(owner(), generation(), before_rev, 900),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    session.verify_move(&post).expect("verify");
    plan
}
fn tree_of(
    session: &Session,
    output: &str,
    workspace: &str,
) -> Option<tiler_core::directional::Node> {
    let k = key(output, workspace);
    session
        .snapshot()
        .domains
        .iter()
        .find(|d| d.output == k.output && d.workspace == k.workspace)
        .and_then(|view| view.tree.clone())
}
fn root_axis(session: &Session, output: &str, workspace: &str) -> Option<Axis> {
    match tree_of(session, output, workspace)? {
        tiler_core::directional::Node::Leaf { .. } => None,
        tiler_core::directional::Node::Group { axis, .. } => Some(axis),
    }
}
fn root_id(session: &Session, output: &str, workspace: &str) -> Option<String> {
    tree_of(session, output, workspace).map(|t| match t {
        tiler_core::directional::Node::Leaf { id } => id.0,
        tiler_core::directional::Node::Group { id, .. } => id.0,
    })
}
/// Direct parent group of `leaf`: (group id, axis, ordered child ids).
fn parent_of(
    session: &Session,
    output: &str,
    workspace: &str,
    leaf: &str,
) -> Option<(String, Axis, Vec<String>)> {
    fn find(
        node: &tiler_core::directional::Node,
        leaf: &str,
    ) -> Option<(String, Axis, Vec<String>)> {
        match node {
            tiler_core::directional::Node::Leaf { .. } => None,
            tiler_core::directional::Node::Group {
                id, axis, children, ..
            } => {
                for child in children {
                    if let tiler_core::directional::Node::Leaf { id: cid } = child
                        && cid.0 == leaf
                    {
                        return Some((
                            id.0.clone(),
                            *axis,
                            children
                                .iter()
                                .map(|c| match c {
                                    tiler_core::directional::Node::Leaf { id } => id.0.clone(),
                                    tiler_core::directional::Node::Group { id, .. } => id.0.clone(),
                                })
                                .collect(),
                        ));
                    }
                    if let Some(found) = find(child, leaf) {
                        return Some(found);
                    }
                }
                None
            }
        }
    }
    find(&tree_of(session, output, workspace)?, leaf)
}
fn focus_commit(
    session: &mut Session,
    domain: &DomainKey,
    window: &tiler_core::directional::WindowId,
    direction: Direction,
    corr: &str,
) {
    let obs = complete_obs(session);
    let plan = session
        .propose_focus(
            domain,
            window,
            direction,
            &obs,
            &correlation(corr),
            &FocusCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("focus {direction:?}: {e:?}"));
    let base = session.accepted_revision();
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
        .verify_focus(&FocusPostObservation::new(
            Observation::new(owner(), generation(), base, 700 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify focus");
}
/// Every desired rectangle is positive, contained in its domain work area,
/// and covers exactly the tiled windows of the listed affected domains.
fn assert_geometry_complete_for(
    plan: &tiler_core::session::SessionMovePlan,
    session: &Session,
    affected: &[(&str, &str)],
) {
    assert!(!plan.desired_geometry.is_empty());
    let mut expected = 0usize;
    for (output, workspace) in affected {
        expected += session
            .snapshot()
            .windows
            .iter()
            .filter(|l| l.output.0 == *output && l.workspace.0 == *workspace)
            .count();
        let domain = session
            .domains()
            .iter()
            .find(|d| d.id.0 == *output && d.workspace.0 == *workspace)
            .expect("affected domain known");
        for g in plan
            .desired_geometry
            .iter()
            .filter(|g| g.output.0 == *output && g.workspace.0 == *workspace)
        {
            assert!(
                g.rect.w > 0 && g.rect.h > 0,
                "positive rect for {}",
                g.window.0
            );
            assert!(
                g.rect.x >= domain.bounds.x
                    && g.rect.y >= domain.bounds.y
                    && g.rect.x + g.rect.w <= domain.bounds.x + domain.bounds.w
                    && g.rect.y + g.rect.h <= domain.bounds.y + domain.bounds.h,
                "rect of {} contained in {output}/{workspace} bounds",
                g.window.0,
            );
        }
    }
    assert_eq!(
        plan.desired_geometry.len(),
        expected,
        "geometry covers exactly the affected tiled windows"
    );
}
fn leaves(session: &Session, output: &str, workspace: &str) -> Vec<String> {
    let k = key(output, workspace);
    match session
        .snapshot()
        .domains
        .iter()
        .find(|d| d.output == k.output && d.workspace == k.workspace)
    {
        Some(view) => match &view.tree {
            Some(tree) => {
                fn collect(t: &tiler_core::directional::Node, out: &mut Vec<String>) {
                    match t {
                        tiler_core::directional::Node::Leaf { id } => out.push(id.0.clone()),
                        tiler_core::directional::Node::Group { children, .. } => {
                            for c in children {
                                collect(c, out);
                            }
                        }
                    }
                }
                let mut out = Vec::new();
                collect(tree, &mut out);
                out
            }
            None => Vec::new(),
        },
        None => panic!("missing domain"),
    }
}

// S20 equivalent with distinct workspaces: L=X, R=H[A,B]; focus A left.
#[test]
fn cross_workspace_s20_occupied_single_leaf() {
    let mut s = two_output_session();
    admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
    admit(&mut s, "win-b", "out-2", "ws-b", "seed-0002");
    // Focus A in out-2: admit order focuses last admitted (win-b); refocus via
    // local focus moves is complex, so drive focus to win-a with a local move
    // if needed. Instead assert via direct propose: move win-b right is noop,
    // move win-b left swaps to focus win-a.
    let k2 = key("out-2", "ws-b");
    // Ensure focused is win-a: if focused is win-b, move left (R2a swap keeps
    // focus on mover win-b though). Use focus API to select win-a locally.
    let current = focused_window(&s, &k2);
    if current.0 != "win-a" {
        // Local focus left from win-b should reach win-a in H[A,B].
        let obs = complete_obs(&s);
        let plan = s
            .propose_focus(
                &k2,
                &current,
                Direction::Left,
                &obs,
                &correlation("focus-a"),
                &FocusCapabilities::full(),
            )
            .expect("local focus to A");
        let base = s.accepted_revision();
        s.acknowledge(&AdapterAck::new(
            correlation("focus-a"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
        let post = FocusPostObservation::new(
            Observation::new(owner(), generation(), base, 901),
            correlation("focus-a"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        s.verify_focus(&post).expect("verify focus");
    }
    assert_eq!(focused_window(&s, &k2).0, "win-a");
    let plan = move_commit(&mut s, &k2, Direction::Left, "move-s20");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::CrossOutput {
            target_output,
            target_workspace,
            target,
            ..
        } => {
            assert_eq!(target_output.0, "out-1");
            assert_eq!(target_workspace.0, "ws-a");
            assert_eq!(
                *target,
                tiler_core::directional::CrossOutputTarget::Occupied
            );
        }
        other => panic!("expected cross-output, got {other:?}"),
    }
    // Target is the adjacent output's current workspace (ws-a), not the same
    // backing index (ws-b).
    assert_eq!(plan.desired_focus_domain, key("out-1", "ws-a"));
    // Source collapses to B; target gains A beside X.
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-b".to_string()]);
    let target_leaves = leaves(&s, "out-1", "ws-a");
    assert_eq!(target_leaves.len(), 2);
    assert!(target_leaves.contains(&"leaf-win-a".to_string()));
    // Complete geometry for source+target with gaps.
    assert!(!plan.desired_geometry.is_empty());
    for g in &plan.desired_geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
    }
}

// S21 equivalent: source holds a perpendicular V[A,B] with A focused at its
// top-left. `Meta+Shift+Left` must apply local R1 inside the source output:
// no CrossOutput operation, no source/target cross-domain changes, and no
// native cross transfer. Mirrors user-tested S21-01 (`R=H[A,B]`, `L` kept).
#[test]
fn cross_workspace_s21_perpendicular_no_cross() {
    let mut s = two_output_session();
    admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
    // Tall placement splits the focused leaf vertically: V[A,B] with A first.
    admit_with_placement(
        &mut s,
        "win-b",
        "out-2",
        "ws-b",
        "seed-0002",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let k1 = key("out-1", "ws-a");
    let k2 = key("out-2", "ws-b");
    assert_eq!(root_axis(&s, "out-2", "ws-b"), Some(Axis::Vertical));
    assert_eq!(
        leaves(&s, "out-2", "ws-b"),
        vec!["leaf-win-a".to_string(), "leaf-win-b".to_string()]
    );
    // Focus A (top edge): Up from B reaches A in the vertical group.
    let focused_b = focused_window(&s, &k2);
    assert_eq!(focused_b.0, "win-b");
    focus_commit(&mut s, &k2, &focused_b, Direction::Up, "s21-focus-a");
    let window_a = focused_window(&s, &k2);
    assert_eq!(window_a.0, "win-a");
    // Capture pre-move state for cross-domain isolation.
    let target_tree_before = tree_of(&s, "out-1", "ws-a");
    let windows_before = s.snapshot().windows.clone();
    let plan = move_commit(&mut s, &k2, Direction::Left, "s21-move");
    // Local R1 only: never a cross-output transfer operation.
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R1);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::WrapPerpendicular {
            container, axis, ..
        } => {
            assert_eq!(*axis, Axis::Horizontal);
            assert!(!container.0.is_empty());
        }
        other => panic!("expected local R1 wrap, got {other:?}"),
    }
    // S21-01 result shape: source becomes H[A,B], target untouched.
    assert_eq!(root_axis(&s, "out-2", "ws-b"), Some(Axis::Horizontal));
    assert_eq!(
        leaves(&s, "out-2", "ws-b"),
        vec!["leaf-win-a".to_string(), "leaf-win-b".to_string()]
    );
    assert_eq!(tree_of(&s, "out-1", "ws-a"), target_tree_before);
    // Focus and membership stay in the source domain: no cross transfer.
    assert_eq!(plan.desired_focus_domain, k2);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-a");
    for link in &s.snapshot().windows {
        let before = windows_before
            .iter()
            .find(|l| l.window == link.window)
            .expect("known window");
        assert_eq!(
            (&link.output, &link.workspace),
            (&before.output, &before.workspace)
        );
    }
    assert!(
        plan.desired_geometry
            .iter()
            .all(|g| g.output.0 == "out-2" && g.workspace.0 == "ws-b")
    );
    assert!(!s.has_pending());
    let _ = k1;
}

// S22 equivalent: empty target with distinct workspaces.
#[test]
fn cross_workspace_s22_empty_target() {
    let mut s = two_output_session();
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
    admit(&mut s, "win-b", "out-2", "ws-b", "seed-0002");
    let k2 = key("out-2", "ws-b");
    // Focus A locally if needed.
    let current = focused_window(&s, &k2);
    if current.0 != "win-a" {
        let obs = complete_obs(&s);
        let plan = s
            .propose_focus(
                &k2,
                &current,
                Direction::Left,
                &obs,
                &correlation("focus-a2"),
                &FocusCapabilities::full(),
            )
            .expect("focus A");
        let base = s.accepted_revision();
        s.acknowledge(&AdapterAck::new(
            correlation("focus-a2"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
        s.verify_focus(&FocusPostObservation::new(
            Observation::new(owner(), generation(), base, 902),
            correlation("focus-a2"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
    }
    // Empty the target domain out-1/ws-a (it starts empty: no admissions).
    assert_eq!(leaves(&s, "out-1", "ws-a"), Vec::<String>::new());
    let plan = move_commit(&mut s, &k2, Direction::Left, "move-s22");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, key("out-1", "ws-a"));
    assert_eq!(leaves(&s, "out-1", "ws-a"), vec!["leaf-win-a".to_string()]);
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-b".to_string()]);
}

// Occupied multiwindow target with distinct workspaces: the target's
// remembered last-focus is the nontrivial nested leaf Z in H[X,V[Y,Z]].
// Crossing A left must insert beside Z (not wrap the whole target tree),
// collapse the source, and follow focus with complete per-domain geometry.
#[test]
fn cross_workspace_occupied_multiwindow_focused_insertion() {
    let mut s = two_output_session();
    // Target out-1/ws-a: X, then Y (wide -> H[X,Y]), then Z (tall -> splits
    // the focused Y vertically). Remembered last-focus becomes leaf-win-z.
    admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
    admit(&mut s, "win-y", "out-1", "ws-a", "seed-0001");
    admit_with_placement(
        &mut s,
        "win-z",
        "out-1",
        "ws-a",
        "seed-0002",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let k1 = key("out-1", "ws-a");
    let target_root_before = root_id(&s, "out-1", "ws-a").expect("target root");
    let z_parent_before = parent_of(&s, "out-1", "ws-a", "leaf-win-z").expect("Z parent");
    assert_ne!(
        z_parent_before.0, target_root_before,
        "Z starts nested, not at root"
    );
    // Source out-2/ws-b: H[A,B] with A focused at the left root edge.
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0003");
    admit(&mut s, "win-b", "out-2", "ws-b", "seed-0004");
    let k2 = key("out-2", "ws-b");
    let focused_b = focused_window(&s, &k2);
    assert_eq!(focused_b.0, "win-b");
    focus_commit(&mut s, &k2, &focused_b, Direction::Left, "mw-focus-a");
    assert_eq!(focused_window(&s, &k2).0, "win-a");
    let plan = move_commit(&mut s, &k2, Direction::Left, "mw-move");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    match &plan.dispatch.operation {
        tiler_core::directional::MoveOperation::CrossOutput {
            target_output,
            target_workspace,
            target,
            ..
        } => {
            assert_eq!(target_output.0, "out-1");
            assert_eq!(target_workspace.0, "ws-a");
            assert_eq!(
                *target,
                tiler_core::directional::CrossOutputTarget::Occupied
            );
        }
        other => panic!("expected cross-output, got {other:?}"),
    }
    // Source collapses to B alone.
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-b".to_string()]);
    // Mover inserted beside Z: new Horizontal pair [Z, A], nested (not root),
    // root identity preserved, remaining target structure unchanged.
    let mover_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-a").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-z".to_string(), "leaf-win-a".to_string()],
        "mover sits nearest the source beside the remembered leaf"
    );
    assert_ne!(
        mover_parent.0,
        root_id(&s, "out-1", "ws-a").expect("target root")
    );
    assert_eq!(
        root_id(&s, "out-1", "ws-a").expect("root"),
        target_root_before
    );
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec![
            "leaf-win-x".to_string(),
            "leaf-win-y".to_string(),
            "leaf-win-z".to_string(),
            "leaf-win-a".to_string()
        ]
    );
    // Desired focus and membership follow the mover into the target domain.
    assert_eq!(plan.desired_focus_domain, k1);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-a");
    let mover_link = s
        .snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == "win-a")
        .expect("mover link")
        .clone();
    assert_eq!(
        (&mover_link.output.0, &mover_link.workspace.0),
        (&"out-1".to_string(), &"ws-a".to_string())
    );
    assert_eq!(s.focus().0, Some(k1));
    // Complete projected geometry across both affected domains/gaps.
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Focus cross-output: exhausted edge selects target last-focus; Up/Down never cross.
#[test]
fn cross_output_focus_selects_target_last_focus() {
    let mut s = two_output_session();
    admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
    admit(&mut s, "win-b", "out-2", "ws-b", "seed-0002");
    // Make out-1 focus X (only window) and out-2 focus B (last admitted).
    // To cross from out-2 to out-1, focus must be at the left root edge (A).
    let k2 = key("out-2", "ws-b");
    let current = focused_window(&s, &k2);
    if current.0 != "win-a" {
        let obs = complete_obs(&s);
        let plan = s
            .propose_focus(
                &k2,
                &current,
                Direction::Left,
                &obs,
                &correlation("ff-a"),
                &FocusCapabilities::full(),
            )
            .expect("focus A");
        let base = s.accepted_revision();
        s.acknowledge(&AdapterAck::new(
            correlation("ff-a"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
        s.verify_focus(&FocusPostObservation::new(
            Observation::new(owner(), generation(), base, 903),
            correlation("ff-a"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
    }
    // Local focus left from A is exhausted (A is left edge of H[A,B]).
    let window_a = focused_window(&s, &k2);
    assert_eq!(window_a.0, "win-a");
    let obs = complete_obs(&s);
    // Local propose_focus refuses Unchanged at the edge.
    assert_eq!(
        s.propose_focus(
            &k2,
            &window_a,
            Direction::Left,
            &obs,
            &correlation("ff-edge"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!s.has_pending());
    // Cross-output focus plans to out-1/ws-a X with no mutation.
    let before_snapshot = s.snapshot();
    // Up/Down never cross (check before staging any pending).
    assert_eq!(
        s.propose_cross_output_focus(
            &k2,
            &window_a,
            Direction::Up,
            &obs,
            &correlation("ff-up"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!s.has_pending());
    let plan = s
        .propose_cross_output_focus(
            &k2,
            &window_a,
            Direction::Left,
            &obs,
            &correlation("ff-cross"),
            &FocusCapabilities::full(),
        )
        .expect("cross focus plans");
    assert_eq!(plan.desired_focus_domain, key("out-1", "ws-a"));
    assert_eq!(
        plan.desired_geometry
            .iter()
            .filter(|g| g.output.0 == "out-1")
            .count(),
        1
    );
    assert_eq!(
        plan.desired_geometry
            .iter()
            .filter(|g| g.output.0 == "out-2")
            .count(),
        2
    );
    // Commit the cross focus: topology unmodified, only focus moves.
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("ff-cross"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_focus(&FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 904),
        correlation("ff-cross"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify cross");
    assert_eq!(s.focus().0, Some(key("out-1", "ws-a")));
    assert_eq!(before_snapshot.domains, s.snapshot().domains);
}

// Refusals: empty target focuses refuse without plan or pending.
#[test]
fn cross_output_focus_empty_target_refuses() {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")], 0),
            domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")], 0),
        ],
    )
    .expect("session");
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
    let k2 = key("out-2", "ws-b");
    let w = focused_window(&s, &k2);
    let obs = complete_obs(&s);
    assert_eq!(
        s.propose_cross_output_focus(
            &k2,
            &w,
            Direction::Left,
            &obs,
            &correlation("e-empty"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!s.has_pending());
}

// Ambiguous duplicate target output domains fail closed at construction:
// adjacency cannot name an output owned by two workspaces.
#[test]
fn cross_output_ambiguous_target_domain_refuses_at_construction() {
    let ambiguous = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")], 0),
            domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")], 0),
            domain_ws("out-2", "ws-c", vec![(Direction::Left, "out-1")], 0),
        ],
    );
    assert!(ambiguous.is_err());
}

// Missing or changed target observation refuses before any plan or pending.
#[test]
fn cross_output_missing_and_changed_target_observation_refuse() {
    let mut s = two_output_session();
    admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
    admit(&mut s, "win-b", "out-2", "ws-b", "seed-0002");
    let k2 = key("out-2", "ws-b");
    let current = focused_window(&s, &k2);
    if current.0 != "win-a" {
        focus_commit(&mut s, &k2, &current, Direction::Left, "ro-focus-a");
    }
    let window_a = focused_window(&s, &k2);
    assert_eq!(window_a.0, "win-a");
    // Missing target window: partial observation, no plan, no pending.
    let full = complete_obs(&s);
    let mut missing = full.windows.clone();
    missing.retain(|w| w.window.0 != "win-x");
    let missing_obs = SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 991),
        windows: missing,
    };
    assert_eq!(
        s.propose_cross_output_focus(
            &k2,
            &window_a,
            Direction::Left,
            &missing_obs,
            &correlation("ro-missing"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert!(!s.has_pending());
    // Changed target visibility (window reported on an unknown workspace):
    // cross-domain mismatch, no plan, no pending.
    let mut changed = full.windows.clone();
    for entry in &mut changed {
        if entry.window.0 == "win-x" {
            entry.workspace = WorkspaceId("ws-ghost".to_owned());
        }
    }
    let changed_obs = SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 992),
        windows: changed,
    };
    assert_eq!(
        s.propose_cross_output_focus(
            &k2,
            &window_a,
            Direction::Left,
            &changed_obs,
            &correlation("ro-changed"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch))
    );
    assert!(!s.has_pending());
    // Same staleness on the move path refuses before planning.
    assert_eq!(
        s.propose_move(
            &k2,
            &window_a,
            Direction::Left,
            &missing_obs,
            &correlation("ro-move-missing"),
            &Capabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert!(!s.has_pending());
}

// Stale owner/revision diverge terminally; nothing commits afterwards.
#[test]
fn cross_output_stale_owner_and_revision_diverge() {
    let setup = || {
        let mut s = two_output_session();
        admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
        admit(&mut s, "win-a", "out-2", "ws-b", "seed-0001");
        s
    };
    let k2 = key("out-2", "ws-b");
    // Wrong owner.
    let mut s = setup();
    let w = focused_window(&s, &k2);
    let bad_owner = OwnerId::parse("owner-9").expect("valid");
    let obs = SessionObservation {
        observation: Observation::new(bad_owner, generation(), s.accepted_revision(), 993),
        windows: complete_obs(&s).windows,
    };
    assert!(matches!(
        s.propose_cross_output_focus(
            &k2,
            &w,
            Direction::Left,
            &obs,
            &correlation("ro-owner"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Diverged(_))
    ));
    assert!(s.divergence().is_some());
    // Stale revision (far ahead of accepted).
    let mut s = setup();
    let w = focused_window(&s, &k2);
    let rev = s.accepted_revision() + 5;
    let obs = SessionObservation {
        observation: Observation::new(owner(), generation(), rev, 994),
        windows: complete_obs(&s).windows,
    };
    assert!(matches!(
        s.propose_cross_output_focus(
            &k2,
            &w,
            Direction::Left,
            &obs,
            &correlation("ro-rev"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Diverged(_))
    ));
    assert!(s.divergence().is_some());
}

// Pending/uncertain guard: while a move is staged, cross proposals refuse
// without touching state; adapter loss then diverges terminally (no retry).
#[test]
fn cross_output_pending_and_uncertain_guards() {
    let mut s = two_output_session();
    admit(&mut s, "win-x", "out-1", "ws-a", "seed-0000");
    admit(&mut s, "win-y", "out-1", "ws-a", "seed-0001");
    admit(&mut s, "win-a", "out-2", "ws-b", "seed-0002");
    let k1 = key("out-1", "ws-a");
    let k2 = key("out-2", "ws-b");
    // Global focus rests on out-2/win-a (last admitted). Shift it to
    // out-1/win-y via cross-output focus: win-a is a lone root leaf, so the
    // local edge is exhausted and the target remembers leaf-win-y.
    let w = window_in(&s, &k2, "win-a");
    let obs = complete_obs(&s);
    let focus_plan = s
        .propose_cross_output_focus(
            &k2,
            &w,
            Direction::Left,
            &obs,
            &correlation("g-focus"),
            &FocusCapabilities::full(),
        )
        .expect("cross focus to out-1");
    assert_eq!(focus_plan.desired_focus_domain, k1);
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("g-focus"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_focus(&FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 995),
        correlation("g-focus"),
        true,
        focus_plan.dispatch.preconditions.clone(),
        focus_plan.dispatch.operation.clone(),
    ))
    .expect("verify cross focus");
    let wy = focused_window(&s, &k1);
    assert_eq!(wy.0, "win-y");
    let obs = complete_obs(&s);
    // Stage a real pending local move (R2a swap of Y left over X).
    let staged = s
        .propose_move(
            &k1,
            &wy,
            Direction::Left,
            &obs,
            &correlation("g-stage"),
            &Capabilities::full(),
        )
        .expect("stage pending");
    assert!(s.has_pending());
    let _ = staged;
    // Uncertain window: further proposals refuse PendingExists, state kept.
    // The cross attempt uses the focused (k1, win-y) Right edge, which would
    // otherwise be cross-eligible toward out-2, so PendingExists is the only
    // reason for refusal.
    assert_eq!(
        s.propose_move(
            &k1,
            &wy,
            Direction::Left,
            &obs,
            &correlation("g-retry"),
            &Capabilities::full()
        ),
        Err(ProposeError::PendingExists)
    );
    assert_eq!(
        s.propose_cross_output_focus(
            &k1,
            &wy,
            Direction::Right,
            &obs,
            &correlation("g-cross"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::PendingExists)
    );
    assert!(s.has_pending());
    // Adapter loss diverges terminally instead of recovering the pending plan.
    let _ = s.note_adapter_loss();
    assert!(s.divergence().is_some());
}
