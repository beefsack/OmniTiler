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
        .expect("move plans")
        .expect_moved("tiled move");
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
// Crossing A left wraps the whole target tree as H[H[X,V[Y,Z]],A] with the
// mover nearest the source, collapse the source, and follow focus with
// complete per-domain geometry.
#[test]
fn cross_workspace_occupied_multiwindow_wraps_whole_target() {
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
    // Whole-target wrap: new Horizontal root [old root, A] with the mover
    // nearest the source (last); nested V[Y,Z] preserved unchanged.
    let mover_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-a").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-1", "ws-a").expect("new root"),
        "mover sits at the new target root"
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-a".to_string()],
        "mover lands last (nearest the eastern source)"
    );
    let z_parent_after = parent_of(&s, "out-1", "ws-a", "leaf-win-z").expect("Z parent");
    assert_eq!(z_parent_after.0, z_parent_before.0);
    assert_eq!(z_parent_after.1, Axis::Vertical);
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
        .expect("stage pending")
        .expect_moved("tiled move");
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

// Far-side fixtures below use the defect logical geometry exactly
// (L (0,0,2048,1152), R (2048,72,1920,1080); scales live adapter-side).
fn defect_two_output_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            OutputDomain {
                id: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-a".to_owned()),
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 2048,
                    h: 1152,
                },
                gap: 0,
                adjacent: [(Direction::Right, OutputId("out-2".to_owned()))]
                    .into_iter()
                    .collect(),
            },
            OutputDomain {
                id: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws-b".to_owned()),
                bounds: Rect {
                    x: 2048,
                    y: 72,
                    w: 1920,
                    h: 1080,
                },
                gap: 0,
                adjacent: [(Direction::Left, OutputId("out-1".to_owned()))]
                    .into_iter()
                    .collect(),
            },
        ],
    )
    .expect("defect two-output session")
}

fn stacked_two_output_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_ws("out-top", "ws-a", vec![(Direction::Down, "out-bottom")], 0),
            domain_ws("out-bottom", "ws-b", vec![(Direction::Up, "out-top")], 0),
        ],
    )
    .expect("stacked two-output session")
}

// Move left from R=H[W3,W4] onto L=H[W1,W2] with far-side W1 remembered:
// the mover must land at the root edge nearest the source, never mid-row.
#[test]
fn r4_left_far_side_remembered_lands_at_root_edge() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-1", "out-1", "ws-a", "far-0000");
    admit(&mut s, "win-2", "out-1", "ws-a", "far-0001");
    let k1 = key("out-1", "ws-a");
    // Remember the far-side (west) leaf: focus W1 before leaving the domain.
    let w2 = window_in(&s, &k1, "win-2");
    focus_commit(&mut s, &k1, &w2, Direction::Left, "far-focus-w1");
    assert_eq!(focused_window(&s, &k1).0, "win-1");
    let target_root_before = root_id(&s, "out-1", "ws-a").expect("target root");
    admit(&mut s, "win-3", "out-2", "ws-b", "far-0002");
    admit(&mut s, "win-4", "out-2", "ws-b", "far-0003");
    let k2 = key("out-2", "ws-b");
    let w4 = focused_window(&s, &k2);
    assert_eq!(w4.0, "win-4");
    focus_commit(&mut s, &k2, &w4, Direction::Left, "far-focus-w3");
    assert_eq!(focused_window(&s, &k2).0, "win-3");
    let plan = move_commit(&mut s, &k2, Direction::Left, "far-move-left");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k1);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-3");
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string()
        ]
    );
    let mover_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-3").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-1", "ws-a").expect("new root"),
        "mover sits at the target root, not nested beside far-side W1"
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()],
        "mover lands last (nearest the eastern source)"
    );
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Mirror: move right from L=H[W3,W4] onto R=H[W1,W2] with far-side W2
// remembered: the mover must land first at the target root, never mid-row.
#[test]
fn r4_right_far_side_remembered_lands_at_root_edge() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-1", "out-2", "ws-b", "far-0000");
    admit(&mut s, "win-2", "out-2", "ws-b", "far-0001");
    // Last-admitted W2 is already the remembered far-side (east) leaf.
    let k2 = key("out-2", "ws-b");
    assert_eq!(focused_window(&s, &k2).0, "win-2");
    let target_root_before = root_id(&s, "out-2", "ws-b").expect("target root");
    admit(&mut s, "win-3", "out-1", "ws-a", "far-0002");
    admit(&mut s, "win-4", "out-1", "ws-a", "far-0003");
    let k1 = key("out-1", "ws-a");
    assert_eq!(focused_window(&s, &k1).0, "win-4");
    let plan = move_commit(&mut s, &k1, Direction::Right, "far-move-right");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k2);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-4");
    assert_eq!(leaves(&s, "out-1", "ws-a"), vec!["leaf-win-3".to_string()]);
    assert_eq!(
        leaves(&s, "out-2", "ws-b"),
        vec![
            "leaf-win-4".to_string(),
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string()
        ]
    );
    let mover_parent = parent_of(&s, "out-2", "ws-b", "leaf-win-4").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-2", "ws-b").expect("new root"),
        "mover sits at the target root, not nested beside far-side W2"
    );
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-4".to_string(), target_root_before],
        "mover lands first (nearest the western source)"
    );
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Full user journey: L=H[W1,W2,W3*], R=W4; move W3 right, then (still
// focused) move it back left. Return must land at the root edge nearest
// the source: L=H[H[W1,W2],W3], R=W4.
#[test]
fn r4_right_then_left_journey_returns_to_root_edge() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-1", "out-1", "ws-a", "j-0000");
    admit(&mut s, "win-2", "out-1", "ws-a", "j-0001");
    admit(&mut s, "win-3", "out-1", "ws-a", "j-0002");
    admit(&mut s, "win-4", "out-2", "ws-b", "j-0003");
    let k1 = key("out-1", "ws-a");
    let k2 = key("out-2", "ws-b");
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string()
        ]
    );
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    // Focus returns to W3 on out-1 via cross-output focus, as the user would.
    let w4 = focused_window(&s, &k2);
    assert_eq!(w4.0, "win-4");
    let obs = complete_obs(&s);
    let focus_plan = s
        .propose_cross_output_focus(
            &k2,
            &w4,
            Direction::Left,
            &obs,
            &correlation("j-focus"),
            &FocusCapabilities::full(),
        )
        .expect("cross focus to W3");
    assert_eq!(focus_plan.desired_focus_leaf.0, "leaf-win-3");
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("j-focus"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_focus(&FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 960),
        correlation("j-focus"),
        true,
        focus_plan.dispatch.preconditions.clone(),
        focus_plan.dispatch.operation.clone(),
    ))
    .expect("verify cross focus");
    assert_eq!(focused_window(&s, &k1).0, "win-3");
    // Session admission nests (H[W1,H[W2,W3]]), so one local R3 escape
    // flattens to H[W1,W2,W3] first; the next press crosses.
    let flat = move_commit(&mut s, &k1, Direction::Right, "j-flatten");
    assert_eq!(flat.dispatch.rule, tiler_core::directional::Rule::R3);
    assert_eq!(focused_window(&s, &k1).0, "win-3");
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string()
        ]
    );
    // Move right: L collapses to H[W1,W2], R gains W3 nearest the source.
    let right = move_commit(&mut s, &k1, Direction::Right, "j-move-right");
    assert_eq!(right.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(right.desired_focus_domain, k2);
    assert_eq!(right.desired_focus_leaf.0, "leaf-win-3");
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
    assert_eq!(
        leaves(&s, "out-2", "ws-b"),
        vec!["leaf-win-3".to_string(), "leaf-win-4".to_string()]
    );
    let r_parent = parent_of(&s, "out-2", "ws-b", "leaf-win-3").expect("mover parent");
    assert_eq!(r_parent.1, Axis::Horizontal);
    assert_eq!(r_parent.0, root_id(&s, "out-2", "ws-b").expect("root"));
    assert_eq!(
        r_parent.2,
        vec!["leaf-win-3".to_string(), "leaf-win-4".to_string()]
    );
    assert_geometry_complete_for(&right, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    // Still focused: move back left onto occupied L.
    assert_eq!(focused_window(&s, &k2).0, "win-3");
    let l_root_mid = root_id(&s, "out-1", "ws-a").expect("mid root");
    let left = move_commit(&mut s, &k2, Direction::Left, "j-move-left");
    assert_eq!(left.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(left.desired_focus_domain, k1);
    assert_eq!(left.desired_focus_leaf.0, "leaf-win-3");
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string()
        ]
    );
    let l_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-3").expect("mover parent");
    assert_eq!(l_parent.1, Axis::Horizontal);
    assert_eq!(l_parent.0, root_id(&s, "out-1", "ws-a").expect("new root"));
    assert_eq!(l_parent.2, vec![l_root_mid, "leaf-win-3".to_string()]);
    assert_geometry_complete_for(&left, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// R4 occupied always wraps the whole target (user decision 2026-10-14):
// Left onto H[W1,W2] gives H[H[W1,W2],W3] with the mover nearest the source,
// regardless of the remembered leaf. Near-side W2 remembered would previously
// nest as H[W1,H[W2,W3]].
#[test]
fn r4_left_near_side_remembered_wraps_whole_target() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-1", "out-1", "ws-a", "near-0000");
    admit(&mut s, "win-2", "out-1", "ws-a", "near-0001");
    // Last-admitted W2 is the remembered near-side (east) leaf.
    let k1 = key("out-1", "ws-a");
    assert_eq!(focused_window(&s, &k1).0, "win-2");
    let target_root_before = root_id(&s, "out-1", "ws-a").expect("target root");
    admit(&mut s, "win-3", "out-2", "ws-b", "near-0002");
    admit(&mut s, "win-4", "out-2", "ws-b", "near-0003");
    let k2 = key("out-2", "ws-b");
    let w4 = focused_window(&s, &k2);
    assert_eq!(w4.0, "win-4");
    focus_commit(&mut s, &k2, &w4, Direction::Left, "near-focus-w3");
    assert_eq!(focused_window(&s, &k2).0, "win-3");
    let plan = move_commit(&mut s, &k2, Direction::Left, "near-move-left");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k1);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-3");
    assert_eq!(leaves(&s, "out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    let mover_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-3").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-1", "ws-a").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()]
    );
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Mirror: Right onto H[W1,W2] with near-side (west) W1 remembered gives
// H[W4,H[W1,W2]], not nested H[H[W4,W1],W2].
#[test]
fn r4_right_near_side_remembered_wraps_whole_target() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-1", "out-2", "ws-b", "rn-0000");
    admit(&mut s, "win-2", "out-2", "ws-b", "rn-0001");
    let k2 = key("out-2", "ws-b");
    // Remember the near-side (west) leaf W1 before leaving the domain.
    let w2 = window_in(&s, &k2, "win-2");
    focus_commit(&mut s, &k2, &w2, Direction::Left, "rn-focus-w1");
    assert_eq!(focused_window(&s, &k2).0, "win-1");
    let target_root_before = root_id(&s, "out-2", "ws-b").expect("target root");
    admit(&mut s, "win-3", "out-1", "ws-a", "rn-0002");
    admit(&mut s, "win-4", "out-1", "ws-a", "rn-0003");
    let k1 = key("out-1", "ws-a");
    // win-4 sits at the right root edge; move right crosses.
    assert_eq!(focused_window(&s, &k1).0, "win-4");
    let plan = move_commit(&mut s, &k1, Direction::Right, "rn-move-right");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k2);
    assert_eq!(plan.desired_focus_leaf.0, "leaf-win-4");
    assert_eq!(leaves(&s, "out-1", "ws-a"), vec!["leaf-win-3".to_string()]);
    let mover_parent = parent_of(&s, "out-2", "ws-b", "leaf-win-4").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-2", "ws-b").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-4".to_string(), target_root_before]
    );
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Nested target preservation: L=H[X,V[Y,Z]] with deep leaf Z remembered;
// moving W left wraps the whole target as H[H[X,V[Y,Z]],W], keeping V[Y,Z]
// intact, never splitting Z.
#[test]
fn r4_left_nested_target_wraps_whole_and_preserves_nesting() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-x", "out-1", "ws-a", "nest-0000");
    admit(&mut s, "win-y", "out-1", "ws-a", "nest-0001");
    admit_with_placement(
        &mut s,
        "win-z",
        "out-1",
        "ws-a",
        "nest-0002",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let k1 = key("out-1", "ws-a");
    // Z is the remembered deep leaf; its parent is nested, not the root.
    let target_root_before = root_id(&s, "out-1", "ws-a").expect("target root");
    let target_tree_before = tree_of(&s, "out-1", "ws-a").expect("target tree");
    let z_parent_before = parent_of(&s, "out-1", "ws-a", "leaf-win-z").expect("Z parent");
    assert_ne!(z_parent_before.0, target_root_before);
    admit(&mut s, "win-3", "out-2", "ws-b", "nest-0003");
    admit(&mut s, "win-4", "out-2", "ws-b", "nest-0004");
    let k2 = key("out-2", "ws-b");
    let w4 = focused_window(&s, &k2);
    focus_commit(&mut s, &k2, &w4, Direction::Left, "nest-focus-w3");
    let plan = move_commit(&mut s, &k2, Direction::Left, "nest-move-left");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k1);
    let mover_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-3").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-1", "ws-a").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()]
    );
    // Nested V[Y,Z] survives unchanged inside the wrapped target.
    let z_parent_after = parent_of(&s, "out-1", "ws-a", "leaf-win-z").expect("Z parent");
    assert_eq!(z_parent_after.0, z_parent_before.0);
    assert_eq!(z_parent_after.1, Axis::Vertical);
    assert_eq!(
        z_parent_after.2,
        vec!["leaf-win-y".to_string(), "leaf-win-z".to_string()]
    );
    // The wrapped child is the original target tree exactly.
    match tree_of(&s, "out-1", "ws-a").expect("new target tree") {
        tiler_core::directional::Node::Group { children, .. } => {
            assert_eq!(children.len(), 2);
            assert_eq!(children[0], target_tree_before);
        }
        other => panic!("expected new root group, got {other:?}"),
    }
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Perpendicular target preservation: L=V[X1,X2]; moving W left wraps as
// H[V[X1,X2],W] on the D axis, never splitting the remembered leaf.
#[test]
fn r4_left_perpendicular_target_wraps_whole() {
    let mut s = defect_two_output_session();
    admit(&mut s, "win-x1", "out-1", "ws-a", "perp-0000");
    admit_with_placement(
        &mut s,
        "win-x2",
        "out-1",
        "ws-a",
        "perp-0001",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let k1 = key("out-1", "ws-a");
    assert_eq!(root_axis(&s, "out-1", "ws-a"), Some(Axis::Vertical));
    let target_root_before = root_id(&s, "out-1", "ws-a").expect("target root");
    admit(&mut s, "win-3", "out-2", "ws-b", "perp-0002");
    admit(&mut s, "win-4", "out-2", "ws-b", "perp-0003");
    let k2 = key("out-2", "ws-b");
    let w4 = focused_window(&s, &k2);
    focus_commit(&mut s, &k2, &w4, Direction::Left, "perp-focus-w3");
    let plan = move_commit(&mut s, &k2, Direction::Left, "perp-move-left");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k1);
    let mover_parent = parent_of(&s, "out-1", "ws-a", "leaf-win-3").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-1", "ws-a").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()]
    );
    // Perpendicular V target keeps its shape as the wrapped child.
    assert_eq!(
        leaves(&s, "out-1", "ws-a"),
        vec![
            "leaf-win-x1".to_string(),
            "leaf-win-x2".to_string(),
            "leaf-win-3".to_string()
        ]
    );
    assert_geometry_complete_for(&plan, &s, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
    assert!(!s.has_pending());
}

// Up onto V[W1,W2]: V[V[W1,W2],W3] with the mover last (nearest the source
// below), for both near-side (south W2) and far-side (north W1) remembered
// leaves.
#[test]
fn r4_up_remembered_leaf_variants_wrap_whole_target() {
    for remembered in ["near", "far"] {
        let mut s = stacked_two_output_session();
        admit(
            &mut s,
            "win-1",
            "out-top",
            "ws-a",
            &format!("up-{remembered}-0000"),
        );
        admit_with_placement(
            &mut s,
            "win-2",
            "out-top",
            "ws-a",
            &format!("up-{remembered}-0001"),
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 120,
            },
        );
        let k_top = key("out-top", "ws-a");
        assert_eq!(root_axis(&s, "out-top", "ws-a"), Some(Axis::Vertical));
        if remembered == "far" {
            // Remember the far-side (north) leaf W1 before leaving the domain.
            let w2 = window_in(&s, &k_top, "win-2");
            focus_commit(
                &mut s,
                &k_top,
                &w2,
                Direction::Up,
                &format!("up-{remembered}-focus-w1"),
            );
            assert_eq!(focused_window(&s, &k_top).0, "win-1");
        } else {
            // Last-admitted W2 is the remembered near-side (south) leaf.
            assert_eq!(focused_window(&s, &k_top).0, "win-2");
        }
        let target_root_before = root_id(&s, "out-top", "ws-a").expect("target root");
        let target_tree_before = tree_of(&s, "out-top", "ws-a").expect("target tree");
        // Sole-leaf source crosses up without local restructure.
        admit(
            &mut s,
            "win-3",
            "out-bottom",
            "ws-b",
            &format!("up-{remembered}-0002"),
        );
        let k_bottom = key("out-bottom", "ws-b");
        let plan = move_commit(
            &mut s,
            &k_bottom,
            Direction::Up,
            &format!("up-{remembered}-move"),
        );
        assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
        assert_eq!(plan.desired_focus_domain, k_top);
        assert_eq!(plan.desired_focus_leaf.0, "leaf-win-3");
        assert_eq!(leaves(&s, "out-bottom", "ws-b"), Vec::<String>::new());
        let mover_parent = parent_of(&s, "out-top", "ws-a", "leaf-win-3").expect("mover parent");
        assert_eq!(mover_parent.1, Axis::Vertical);
        assert_eq!(
            mover_parent.0,
            root_id(&s, "out-top", "ws-a").expect("new root")
        );
        assert_eq!(
            mover_parent.2,
            vec![target_root_before, "leaf-win-3".to_string()],
            "mover lands last (nearest the southern source), remembered {remembered}"
        );
        // The wrapped child is the original target tree exactly.
        match tree_of(&s, "out-top", "ws-a").expect("new target tree") {
            tiler_core::directional::Node::Group { children, .. } => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0], target_tree_before);
            }
            other => panic!("expected new root group, got {other:?}"),
        }
        assert_geometry_complete_for(&plan, &s, &[("out-top", "ws-a"), ("out-bottom", "ws-b")]);
        assert!(!s.has_pending());
    }
}

// Down onto V[W1,W2]: V[W3,V[W1,W2]] with the mover first (nearest the
// source above), for both near-side (north W1) and far-side (south W2)
// remembered leaves.
#[test]
fn r4_down_remembered_leaf_variants_wrap_whole_target() {
    for remembered in ["near", "far"] {
        let mut s = stacked_two_output_session();
        admit(
            &mut s,
            "win-1",
            "out-bottom",
            "ws-b",
            &format!("dn-{remembered}-0000"),
        );
        admit_with_placement(
            &mut s,
            "win-2",
            "out-bottom",
            "ws-b",
            &format!("dn-{remembered}-0001"),
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 120,
            },
        );
        let k_bottom = key("out-bottom", "ws-b");
        assert_eq!(root_axis(&s, "out-bottom", "ws-b"), Some(Axis::Vertical));
        if remembered == "near" {
            // Remember the near-side (north) leaf W1 before leaving the domain.
            let w2 = window_in(&s, &k_bottom, "win-2");
            focus_commit(
                &mut s,
                &k_bottom,
                &w2,
                Direction::Up,
                &format!("dn-{remembered}-focus-w1"),
            );
            assert_eq!(focused_window(&s, &k_bottom).0, "win-1");
        } else {
            // Last-admitted W2 is the remembered far-side (south) leaf.
            assert_eq!(focused_window(&s, &k_bottom).0, "win-2");
        }
        let target_root_before = root_id(&s, "out-bottom", "ws-b").expect("target root");
        admit(
            &mut s,
            "win-3",
            "out-top",
            "ws-a",
            &format!("dn-{remembered}-0002"),
        );
        let k_top = key("out-top", "ws-a");
        let plan = move_commit(
            &mut s,
            &k_top,
            Direction::Down,
            &format!("dn-{remembered}-move"),
        );
        assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
        assert_eq!(plan.desired_focus_domain, k_bottom);
        assert_eq!(plan.desired_focus_leaf.0, "leaf-win-3");
        assert_eq!(leaves(&s, "out-top", "ws-a"), Vec::<String>::new());
        let mover_parent = parent_of(&s, "out-bottom", "ws-b", "leaf-win-3").expect("mover parent");
        assert_eq!(mover_parent.1, Axis::Vertical);
        assert_eq!(
            mover_parent.0,
            root_id(&s, "out-bottom", "ws-b").expect("new root")
        );
        assert_eq!(
            mover_parent.2,
            vec!["leaf-win-3".to_string(), target_root_before],
            "mover lands first (nearest the northern source), remembered {remembered}"
        );
        assert_geometry_complete_for(&plan, &s, &[("out-top", "ws-a"), ("out-bottom", "ws-b")]);
        assert!(!s.has_pending());
    }
}

// Vertical onto a perpendicular target: Down onto H[X1,X2] wraps the whole
// target as V[W,H[X1,X2]] on the D axis.
#[test]
fn r4_down_perpendicular_target_wraps_whole() {
    let mut s = stacked_two_output_session();
    admit(&mut s, "win-x1", "out-bottom", "ws-b", "vp-0000");
    admit(&mut s, "win-x2", "out-bottom", "ws-b", "vp-0001");
    let k_bottom = key("out-bottom", "ws-b");
    assert_eq!(root_axis(&s, "out-bottom", "ws-b"), Some(Axis::Horizontal));
    let target_root_before = root_id(&s, "out-bottom", "ws-b").expect("target root");
    admit(&mut s, "win-3", "out-top", "ws-a", "vp-0002");
    let k_top = key("out-top", "ws-a");
    let plan = move_commit(&mut s, &k_top, Direction::Down, "vp-move");
    assert_eq!(plan.dispatch.rule, tiler_core::directional::Rule::R4);
    assert_eq!(plan.desired_focus_domain, k_bottom);
    let mover_parent = parent_of(&s, "out-bottom", "ws-b", "leaf-win-3").expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Vertical);
    assert_eq!(
        mover_parent.0,
        root_id(&s, "out-bottom", "ws-b").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-3".to_string(), target_root_before]
    );
    assert_geometry_complete_for(&plan, &s, &[("out-top", "ws-a"), ("out-bottom", "ws-b")]);
    assert!(!s.has_pending());
}
