#![forbid(unsafe_code)]
//! Session/domain lifecycle integration tests (portable, headless).
//!
//! Drives the real `session::Session` API through full
//! propose/acknowledge/verify cycles. No live compositor state.

use tiler_core::contract::{
    AckOutcome, AdapterAck, DivergenceKind, LIFECYCLE_POLICY_VERSION, LifecycleCapabilities,
    LifecyclePostObservation, Observation,
};
use tiler_core::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionBehavior, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError,
    RefusalKind, Session, SessionCommand, SessionObservation, SessionPlan,
};
use tiler_core::size_hints::WindowSizeHints;

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
        adjacent: std::collections::BTreeMap::new(),
    }
}

fn single_domain_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 120, 80)],
    )
    .expect("session")
}

fn two_domain_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("out-1", "ws-1", 120, 80),
            domain("out-2", "ws-2", 120, 80),
        ],
    )
    .expect("session")
}

fn placement(w: i32, h: i32) -> Rect {
    Rect { x: 0, y: 0, w, h }
}

fn tiled_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
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
        hints: WindowSizeHints::none(),
    }
}

fn exception_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

/// Complete pre-observation for the session state plus optional extra entries.
/// Revision always tracks the session accepted revision.
fn complete_observation(session: &Session, extra: Vec<ObservedWindow>) -> SessionObservation {
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| ObservedWindow {
            window: link.window.clone(),
            output: link.output.clone(),
            workspace: link.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        })
        .collect();
    windows.extend(session.exception_observed());
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
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

fn admit_command(window: &str, output: &str, workspace: &str, placement: Rect) -> SessionCommand {
    SessionCommand::Admit {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: placement,
        suppress_fixed_float: false,
    }
}

/// Full transactional cycle: propose, acknowledge, verify. Returns the plan.
fn admit_and_commit(
    session: &mut Session,
    window: &str,
    output: &str,
    workspace: &str,
    placement: Rect,
    corr: &str,
) -> SessionPlan {
    let extra = vec![tiled_observed(window, output, workspace)];
    let obs = complete_observation(session, extra);
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &admit_command(window, output, workspace, placement),
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("admit {window} proposes: {e:?}"));
    assert_eq!(plan.dispatch.base_revision, base);
    assert_eq!(plan.dispatch.correlation_id.as_str(), corr);
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            tiler_core::contract::AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    let commit = session.verify_lifecycle(&post).expect("commit");
    assert_eq!(commit.revision, base + 1);
    plan
}

fn remove_and_commit(session: &mut Session, window: &str, corr: &str) -> SessionPlan {
    let obs = complete_observation(session, Vec::new());
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &SessionCommand::Remove {
                window: WindowId(window.to_owned()),
            },
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("remove {window} proposes: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            tiler_core::contract::AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 300 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    session.verify_lifecycle(&post).expect("commit");
    plan
}

fn toggle_float_and_commit(
    session: &mut Session,
    window: &str,
    float_geometry: Option<Rect>,
    corr: &str,
) -> SessionPlan {
    let obs = complete_observation(session, Vec::new());
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId(window.to_owned()),
                float_geometry,
            },
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("toggle-float {window} proposes: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            tiler_core::contract::AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 600 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    session.verify_lifecycle(&post).expect("commit");
    plan
}

fn leaves_of(session: &Session, output: &str, workspace: &str) -> Vec<String> {
    let snapshot = session.snapshot();
    let view = snapshot
        .domains
        .iter()
        .find(|d| d.output.0 == output && d.workspace.0 == workspace)
        .expect("domain");
    match &view.tree {
        None => Vec::new(),
        Some(tree) => {
            let mut ids = Vec::new();
            collect(tree, &mut ids);
            ids
        }
    }
}

fn domain_key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn collect(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Leaf { id } => out.push(id.0.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect(child, out);
            }
        }
    }
}

fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn initial_admit_makes_root_leaf_focuses_and_projects() {
    let mut session = single_domain_session();
    let plan = admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    let (focus_domain, focus_leaf) = session.focus();
    assert_eq!(focus_domain, Some(domain_key("out-1", "ws-1")));
    assert_eq!(focus_leaf, Some(NodeId("leaf-win-1".to_owned())));
    // Complete geometry for the affected tiled window, positive and contained.
    assert_eq!(plan.desired_geometry.len(), 1);
    let rect = plan.desired_geometry[0].rect;
    assert!(rect.w > 0 && rect.h > 0);
    assert_eq!(
        rect,
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80
        }
    );
    assert_eq!(session.accepted_revision(), 1);
}

#[test]
fn sibling_insert_after_focus_when_axes_match() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    // Second admit wraps the focused root leaf old/new: group H [leaf-1, leaf-2].
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
    match &session.snapshot().domains[0].tree {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            assert_eq!(children[1].id().0, "leaf-win-2");
        }
        other => panic!("expected focused-root binary wrapper, got {other:?}"),
    }
    // Third admit with horizontal placement matches parent axis but must not
    // append: automatic admission wraps the focused leaf old/new.
    let plan = admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-3",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    let snapshot = session.snapshot();
    match &snapshot.domains[0].tree {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            // Rejects same-axis N-ary append: root stays binary.
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected focused binary wrapper old/new, got {other:?}"),
            }
        }
        other => panic!("expected binary root wrapper, got {other:?}"),
    }
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-3".to_owned())));
    assert_eq!(plan.desired_geometry.len(), 3);
    for entry in &plan.desired_geometry {
        assert!(entry.rect.w > 0 && entry.rect.h > 0);
    }
    // Pairwise non-overlap and containment.
    for i in 0..plan.desired_geometry.len() {
        let outer = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        };
        let r = plan.desired_geometry[i].rect;
        assert!(r.x >= outer.x && r.y >= outer.y);
        assert!(r.x + r.w <= outer.x + outer.w && r.y + r.h <= outer.y + outer.h);
        for j in (i + 1)..plan.desired_geometry.len() {
            assert!(
                !rects_intersect(&r, &plan.desired_geometry[j].rect),
                "overlap"
            );
        }
    }
}

#[test]
fn no_focus_root_wraps_entire_root() {
    let mut session = two_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    // Move global focus to the other domain so out-1/ws-1 has no eligible focus.
    admit_and_commit(
        &mut session,
        "win-2",
        "out-2",
        "ws-2",
        placement(120, 80),
        "corr-2",
    );
    // No eligible focus in out-1/ws-1 but a root leaf exists: wrap the entire
    // root old/new, never append.
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-3",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string(), "leaf-win-3".to_string()]
    );
    let snapshot = session.snapshot();
    let view = snapshot
        .domains
        .iter()
        .find(|d| d.output.0 == "out-1" && d.workspace.0 == "ws-1")
        .expect("domain");
    match &view.tree {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            assert_eq!(children[1].id().0, "leaf-win-3");
        }
        other => panic!("expected no-focus root binary wrapper old/new, got {other:?}"),
    }
    // Untouched domain stays isolated and focus follows the new leaf.
    assert_eq!(
        leaves_of(&session, "out-2", "ws-2"),
        vec!["leaf-win-2".to_string()]
    );
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-3".to_owned())));
}

#[test]
fn nesting_when_axes_differ_uses_input_orientation() {
    let mut session = single_domain_session();
    // Wide targets select portable Horizontal splits under the COSMIC
    // admission rule (source Vertical splits width).
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
    // Root is now Horizontal; admit with tall (vertical-selecting) bounds
    // nests focused leaf.
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(80, 120),
        "corr-3",
    );
    let snapshot = session.snapshot();
    match &snapshot.domains[0].tree {
        Some(Node::Group {
            id,
            axis,
            children,
            shares,
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected nested vertical group, got {other:?}"),
            }
            let _ = id;
        }
        other => panic!("expected root group, got {other:?}"),
    }
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-3".to_owned())));
}

#[test]
fn vertical_selected_on_tie() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(100, 100),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(100, 100),
        "corr-2",
    );
    let snapshot = session.snapshot();
    match &snapshot.domains[0].tree {
        Some(Node::Group { axis, .. }) => assert_eq!(*axis, Axis::Vertical),
        other => panic!("tie must select vertical, got {other:?}"),
    }
}

#[test]
fn focused_removal_uses_source_focus_recency_not_tree_sibling() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-3",
    );
    let domain = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    assert!(session.sync_focus_from_window(&domain, &WindowId("win-1".to_owned())));
    assert!(session.sync_focus_from_window(&domain, &WindowId("win-2".to_owned())));
    // Tree order would choose win-3, but the source focus stack's top after
    // removing win-2 is win-1.
    remove_and_commit(&mut session, "win-2", "corr-4");
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string(), "leaf-win-3".to_string()]
    );
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-1".to_owned())));
    // Removing an unfocused window preserves the active source focus.
    remove_and_commit(&mut session, "win-3", "corr-5");
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-1".to_owned())));
}

#[test]
fn removal_prunes_unfocused_leaves_from_focus_recency() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-3",
    );
    // Focus win-3. Remove win-1 (non-focused): focus stays win-3 and win-1
    // cannot be selected from recency later.
    remove_and_commit(&mut session, "win-1", "corr-4");
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-3".to_owned())));
    // Removing focused win-3 now selects the remaining MRU entry, win-2.
    remove_and_commit(&mut session, "win-3", "corr-5");
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-2".to_owned())));
}

#[test]
fn removal_retains_empty_domains() {
    let mut session = two_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-other",
        "out-2",
        "ws-2",
        placement(120, 80),
        "corr-other",
    );
    let source = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    assert!(session.sync_focus_from_window(&source, &WindowId("win-1".to_owned())));
    remove_and_commit(&mut session, "win-1", "corr-2");
    let snapshot = session.snapshot();
    assert_eq!(snapshot.domains.len(), 2);
    assert!(snapshot.domains[0].tree.is_none());
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(session.focus(), (None, None));
    // Domain still usable: admit again.
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-3",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-2".to_string()]
    );
}

#[test]
fn nested_collapse_is_recursive() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
    // Nest win-3 under win-2 with vertical orientation.
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(80, 120),
        "corr-3",
    );
    // Remove win-3: inner vertical group collapses back to leaf win-2.
    remove_and_commit(&mut session, "win-3", "corr-4");
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
    let snapshot = session.snapshot();
    match &snapshot.domains[0].tree {
        Some(Node::Group { children, .. }) => assert_eq!(children.len(), 2),
        other => panic!("expected collapsed root group, got {other:?}"),
    }
}

#[test]
fn output_workspace_isolation() {
    let mut session = two_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-2",
        "ws-2",
        placement(120, 80),
        "corr-2",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    assert_eq!(
        leaves_of(&session, "out-2", "ws-2"),
        vec!["leaf-win-2".to_string()]
    );
    remove_and_commit(&mut session, "win-1", "corr-3");
    // out-2 untouched; out-1 domain retained but empty.
    assert!(leaves_of(&session, "out-1", "ws-1").is_empty());
    assert_eq!(
        leaves_of(&session, "out-2", "ws-2"),
        vec!["leaf-win-2".to_string()]
    );
    let snapshot = session.snapshot();
    assert_eq!(snapshot.domains.len(), 2);
}

#[test]
fn stale_revision_diverges_terminal() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    let mut bad_obs =
        complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    bad_obs.observation = Observation::new(owner(), generation(), 99, 99);
    let err = session
        .propose(
            &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
            &bad_obs,
            &correlation("corr-2"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("stale must diverge");
    assert_eq!(err, ProposeError::Diverged(DivergenceKind::StaleRevision));
    // Terminal.
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    assert_eq!(
        session.propose(
            &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
            &obs,
            &correlation("corr-3"),
            &LifecycleCapabilities::full(),
        ),
        Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
    );
}

#[test]
fn invalid_capability_diverges() {
    let mut session = single_domain_session();
    let obs = complete_observation(&session, vec![tiled_observed("win-1", "out-1", "ws-1")]);
    let err = session
        .propose(
            &admit_command("win-1", "out-1", "ws-1", placement(120, 80)),
            &obs,
            &correlation("corr-1"),
            &LifecycleCapabilities::none(),
        )
        .expect_err("capability must diverge");
    assert_eq!(
        err,
        ProposeError::Diverged(DivergenceKind::CapabilityRefused)
    );
}

#[test]
fn at_most_one_pending() {
    let mut session = single_domain_session();
    let obs = complete_observation(&session, vec![tiled_observed("win-1", "out-1", "ws-1")]);
    session
        .propose(
            &admit_command("win-1", "out-1", "ws-1", placement(120, 80)),
            &obs,
            &correlation("corr-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("first");
    let obs2 = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    assert_eq!(
        session.propose(
            &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
            &obs2,
            &correlation("corr-2"),
            &LifecycleCapabilities::full(),
        ),
        Err(ProposeError::PendingExists)
    );
    assert!(session.has_pending());
    assert_eq!(session.divergence(), None);
}

#[test]
fn duplicate_unknown_crossdomain_partial_malformed_refused_without_divergence() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    // Duplicate.
    let obs = complete_observation(&session, vec![tiled_observed("win-1", "out-1", "ws-1")]);
    assert_eq!(
        session
            .propose(
                &admit_command("win-1", "out-1", "ws-1", placement(120, 80)),
                &obs,
                &correlation("corr-2"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("duplicate"),
        ProposeError::Refused(RefusalKind::DuplicateWindow)
    );
    // Unknown removal.
    let obs = complete_observation(&session, Vec::new());
    assert_eq!(
        session
            .propose(
                &SessionCommand::Remove {
                    window: WindowId("nope".to_owned()),
                },
                &obs,
                &correlation("corr-3"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("unknown"),
        ProposeError::Refused(RefusalKind::UnknownWindow)
    );
    // Cross-domain: unknown output.
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-9", "ws-9")]);
    assert_eq!(
        session
            .propose(
                &admit_command("win-2", "out-9", "ws-9", placement(120, 80)),
                &obs,
                &correlation("corr-4"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("cross-domain"),
        ProposeError::Refused(RefusalKind::CrossDomainMismatch)
    );
    // Cross-domain: workspace mismatch.
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-9")]);
    assert_eq!(
        session
            .propose(
                &admit_command("win-2", "out-1", "ws-9", placement(120, 80)),
                &obs,
                &correlation("corr-5"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("workspace mismatch"),
        ProposeError::Refused(RefusalKind::CrossDomainMismatch)
    );
    // Partial: omit known win-1 from observed while admitting win-2.
    let partial = SessionObservation {
        observation: Observation::new(owner(), generation(), session.accepted_revision(), 50),
        windows: vec![tiled_observed("win-2", "out-1", "ws-1")],
    };
    assert_eq!(
        session
            .propose(
                &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
                &partial,
                &correlation("corr-6"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("partial"),
        ProposeError::Refused(RefusalKind::PartialObservation)
    );
    // Malformed: empty window id.
    let obs = complete_observation(&session, Vec::new());
    assert_eq!(
        session
            .propose(
                &SessionCommand::Remove {
                    window: WindowId(String::new()),
                },
                &obs,
                &correlation("corr-7"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("malformed"),
        ProposeError::Refused(RefusalKind::MalformedInput)
    );
    // Malformed placement bounds.
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    assert_eq!(
        session
            .propose(
                &admit_command("win-2", "out-1", "ws-1", placement(0, 80)),
                &obs,
                &correlation("corr-8"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("bad bounds"),
        ProposeError::Refused(RefusalKind::MalformedInput)
    );
    // Still usable after refusals.
    assert_eq!(session.divergence(), None);
    assert!(!session.has_pending());
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-9",
    );
}

#[test]
fn exception_flags_fail_closed_until_behavior_selected() {
    let mut session = single_domain_session();
    // Floating without behavior: fail closed.
    let obs = complete_observation(&session, vec![exception_observed("win-9", "out-1", "ws-1")]);
    let floating = SessionCommand::Admit {
        window: WindowId("win-9".to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        exceptions: ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        exception_behavior: None,
        placement_bounds: placement(120, 80),
        suppress_fixed_float: false,
    };
    assert_eq!(
        session
            .propose(
                &floating,
                &obs,
                &correlation("corr-1"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("must fail closed"),
        ProposeError::Refused(RefusalKind::ExceptionBehaviorUnselected)
    );
    // With explicit deferral: tracked as exception, no topology effect.
    let deferred = SessionCommand::Admit {
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
        placement_bounds: placement(120, 80),
        suppress_fixed_float: false,
    };
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &deferred,
            &obs,
            &correlation("corr-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("deferred proposes");
    assert!(plan.desired_geometry.is_empty());
    assert!(plan.desired_snapshot.windows.is_empty());
    session
        .acknowledge(&AdapterAck::new(
            correlation("corr-2"),
            owner(),
            generation(),
            base,
            tiler_core::contract::AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 210),
            correlation("corr-2"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
    assert!(session.is_exception(&WindowId("win-9".to_owned())));
    assert!(leaves_of(&session, "out-1", "ws-1").is_empty());
    // Tiled admission still works alongside the exception.
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-3",
    );
    assert_eq!(session.exception_count(), 1);
}

#[test]
fn intentional_float_is_transactional_non_tree_state_and_unfloats_by_fresh_admission() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "float-admit",
    );
    let float_rect = Rect {
        x: 24,
        y: 16,
        w: 72,
        h: 48,
    };
    let base = session.accepted_revision();
    let float = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId("win-1".to_owned()),
                float_geometry: Some(float_rect),
            },
            &complete_observation(&session, Vec::new()),
            &correlation("float-off"),
            &LifecycleCapabilities::full(),
        )
        .expect("tiled window floats");
    assert!(float.desired_geometry.is_empty());
    session
        .acknowledge(&AdapterAck::new(
            correlation("float-off"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("float ack");
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 401),
            correlation("float-off"),
            true,
            float.dispatch.preconditions.clone(),
            float.dispatch.operation.clone(),
        ))
        .expect("float commit");
    assert!(session.snapshot().windows.is_empty());
    let record = session.exception_observed();
    assert_eq!(record.len(), 1);
    assert!(record[0].floating);
    assert_eq!(
        session.floating_geometry(&WindowId("win-1".to_owned())),
        Some(float_rect)
    );

    let base = session.accepted_revision();
    let tiled = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId("win-1".to_owned()),
                float_geometry: None,
            },
            &complete_observation(&session, Vec::new()),
            &correlation("float-on"),
            &LifecycleCapabilities::full(),
        )
        .expect("floating window is freshly admitted");
    assert_eq!(tiled.desired_geometry.len(), 1);
    session
        .acknowledge(&AdapterAck::new(
            correlation("float-on"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("unfloat ack");
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 402),
            correlation("float-on"),
            true,
            tiled.dispatch.preconditions.clone(),
            tiled.dispatch.operation.clone(),
        ))
        .expect("unfloat commit");
    assert_eq!(leaves_of(&session, "out-1", "ws-1"), vec!["leaf-win-1"]);
    assert_eq!(session.exception_count(), 0);
    // The float placement survives unfloat as durable retained geometry.
    assert_eq!(
        session.retained_float_geometry(&WindowId("win-1".to_owned())),
        Some(float_rect)
    );
}

#[test]
fn intentional_float_retains_moved_geometry_across_float_unfloat_float() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "retain-admit",
    );
    let first = Rect {
        x: 24,
        y: 16,
        w: 72,
        h: 48,
    };
    toggle_float_and_commit(&mut session, "win-1", Some(first), "retain-float-1");
    assert_eq!(
        session.floating_geometry(&WindowId("win-1".to_owned())),
        Some(first)
    );

    // The user moves/resizes the float; unfloat carries the live rect.
    let moved = Rect {
        x: 30,
        y: 20,
        w: 50,
        h: 40,
    };
    toggle_float_and_commit(&mut session, "win-1", Some(moved), "retain-unfloat");
    assert_eq!(session.exception_count(), 0);
    assert_eq!(
        session.retained_float_geometry(&WindowId("win-1".to_owned())),
        Some(moved)
    );

    // Re-float selects the retained placement instead of recomputing center.
    toggle_float_and_commit(&mut session, "win-1", None, "retain-float-2");
    assert_eq!(
        session.floating_geometry(&WindowId("win-1".to_owned())),
        Some(moved)
    );
    assert_eq!(
        session.retained_float_geometry(&WindowId("win-1".to_owned())),
        Some(moved)
    );

    // Fresh admission after a second unfloat still uses domain bounds, not the
    // retained float rectangle.
    let unfloated = toggle_float_and_commit(&mut session, "win-1", Some(moved), "retain-unfloat-2");
    assert_eq!(unfloated.desired_geometry.len(), 1);
    assert_eq!(leaves_of(&session, "out-1", "ws-1"), vec!["leaf-win-1"]);
    assert_eq!(session.exception_count(), 0);
}

#[test]
fn intentional_float_centered_fallback_matches_adapter_placement() {
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "center-admit",
    );
    // No explicit rect and no retained geometry: the session resolves the
    // centered 60% work-area placement itself.
    toggle_float_and_commit(&mut session, "win-1", None, "center-float");
    assert_eq!(
        session.floating_geometry(&WindowId("win-1".to_owned())),
        Some(Rect {
            x: 24,
            y: 16,
            w: 72,
            h: 48
        })
    );
}

#[test]
fn unfloat_reuses_admission_axis_not_domain_bounds() {
    // Landscape 120x80 work area. Build H[win-1 V[win-2 win-3]] the same way
    // ordinary admission does (wide admits split H, tall nests V), then float
    // win-3 and unfloat it. Fresh admission must reuse the exact new-window
    // placement rule (focused leaf's projected rect, else domain bounds), so
    // win-3 nests back under win-2 instead of flattening to H[win-1 win-2
    // win-3] from the wide domain bounds. No remembered-origin slot.
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "axis-admit-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "axis-admit-2",
    );
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(80, 120),
        "axis-admit-3",
    );
    assert_nested_h_v(&session);

    toggle_float_and_commit(&mut session, "win-3", None, "axis-float");
    assert_eq!(session.exception_count(), 1);
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
    match &session.snapshot().domains[0].tree {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            assert_eq!(children[1].id().0, "leaf-win-2");
        }
        other => panic!("expected flat H[win-1 win-2] after float, got {other:?}"),
    }
    let retained = session
        .retained_float_geometry(&WindowId("win-3".to_owned()))
        .expect("float retains placement");

    toggle_float_and_commit(&mut session, "win-3", None, "axis-unfloat");
    assert_eq!(session.exception_count(), 0);
    assert_nested_h_v(&session);
    // The float placement survives unfloat as durable retained geometry while
    // the tree itself is freshly admitted.
    assert_eq!(
        session.retained_float_geometry(&WindowId("win-3".to_owned())),
        Some(retained)
    );
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-3".to_owned())));
}

fn assert_nested_h_v(session: &Session) {
    assert_eq!(
        leaves_of(session, "out-1", "ws-1"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    match &session.snapshot().domains[0].tree {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group { axis, children, .. } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected inner V[win-2 win-3], got {other:?}"),
            }
        }
        other => panic!("expected root H[win-1 V[win-2 win-3]], got {other:?}"),
    }
}

#[test]
fn deterministic_replay() {
    fn run() -> (Session, Vec<SessionPlan>) {
        let mut session = single_domain_session();
        let plans = vec![
            admit_and_commit(
                &mut session,
                "win-1",
                "out-1",
                "ws-1",
                placement(120, 80),
                "corr-1",
            ),
            admit_and_commit(
                &mut session,
                "win-2",
                "out-1",
                "ws-1",
                placement(120, 80),
                "corr-2",
            ),
            admit_and_commit(
                &mut session,
                "win-3",
                "out-1",
                "ws-1",
                placement(80, 120),
                "corr-3",
            ),
        ];
        (session, plans)
    }
    let (a_session, a_plans) = run();
    let (b_session, b_plans) = run();
    assert_eq!(a_session.snapshot(), b_session.snapshot());
    assert_eq!(a_session.focus(), b_session.focus());
    assert_eq!(a_plans, b_plans);
}

#[test]
fn tiny_bounds_projection_fails_closed() {
    // Narrow domain with portable Horizontal splits (wide placements):
    // the ordinary anchor cannot host a third horizontal segment in
    // width 2, so the D2 largest fallback splits the other 1x10 column
    // vertically instead of holding (best effort, selected 2026-10-17).
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 2, 10)],
    )
    .expect("session");
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(10, 2),
        "corr-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(10, 2),
        "corr-2",
    );
    // The ordinary anchor wraps horizontally and fails, but the largest
    // fallback (win-1, first in DFS order on the equal-area tie) splits
    // vertically into two positive 1x5 tiles.
    let obs = complete_observation(&session, vec![tiled_observed("win-3", "out-1", "ws-1")]);
    let plan = session
        .propose(
            &admit_command("win-3", "out-1", "ws-1", placement(10, 2)),
            &obs,
            &correlation("corr-3"),
            &LifecycleCapabilities::full(),
        )
        .expect("fallback tiles instead of holding");
    assert_eq!(plan.desired_geometry.len(), 3);
    assert!(
        plan.desired_geometry
            .iter()
            .all(|g| g.rect.w > 0 && g.rect.h > 0),
        "every tile stays positive"
    );
    assert_eq!(
        session.divergence(),
        None,
        "fallback admission never diverges"
    );
}

#[test]
fn owner_mismatch_diverges() {
    let mut session = single_domain_session();
    let mut obs = complete_observation(&session, vec![tiled_observed("win-1", "out-1", "ws-1")]);
    obs.observation = Observation::new(
        OwnerId::parse("owner-2").expect("valid"),
        generation(),
        session.accepted_revision(),
        99,
    );
    assert_eq!(
        session
            .propose(
                &admit_command("win-1", "out-1", "ws-1", placement(120, 80)),
                &obs,
                &correlation("corr-1"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("owner mismatch"),
        ProposeError::Diverged(DivergenceKind::OwnerMismatch)
    );
}

#[test]
fn dispatch_binds_identity_intent_capability_and_affected_ids() {
    let mut session = single_domain_session();
    let plan = admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    assert_eq!(plan.dispatch.correlation_id.as_str(), "corr-1");
    assert_eq!(plan.dispatch.owner.as_str(), "owner-1");
    assert_eq!(plan.dispatch.generation.as_str(), "gen-1");
    assert_eq!(plan.dispatch.base_revision, 0);
    match &plan.dispatch.intent {
        tiler_core::contract::LifecycleIntent::Admit {
            window,
            output,
            workspace,
        } => {
            assert_eq!(window.0, "win-1");
            assert_eq!(output.0, "out-1");
            assert_eq!(workspace.0, "ws-1");
        }
        other => panic!("expected admit intent, got {other:?}"),
    }
    assert!(
        plan.dispatch.preconditions.contains(
            &tiler_core::contract::LifecyclePrecondition::AdapterMustVerifyPostconditions
        )
    );
    // Portable lifecycle policy binding: explicit cosmic_v1 version.
    assert_eq!(plan.dispatch.policy_version, LIFECYCLE_POLICY_VERSION);
    assert_eq!(plan.dispatch.policy_version, 1);
    // Domain-scoped focus and geometry carry the exact domain pair.
    assert_eq!(plan.desired_focus_domain, Some(domain_key("out-1", "ws-1")));
    assert_eq!(plan.desired_geometry[0].output.0, "out-1");
    assert_eq!(plan.desired_geometry[0].workspace.0, "ws-1");
    // Desired snapshot carries the affected leaf; geometry is complete.
    assert_eq!(plan.desired_snapshot.windows.len(), 1);
    assert_eq!(plan.desired_geometry.len(), 1);
    assert_eq!(plan.desired_geometry[0].window.0, "win-1");
    // No platform-native terms leak into the portable plan debug rendering.
    let rendered = format!("{:?}", plan.dispatch.operation);
    assert!(!rendered.contains("pixel"));
    assert!(!rendered.contains("handle"));
    assert!(!rendered.contains("kwin"));
}

#[test]
fn errors_are_redacted_and_bounded() {
    assert_eq!(ProposeError::PendingExists.kind(), "pending-exists");
    assert_eq!(
        ProposeError::Refused(RefusalKind::DuplicateWindow).kind(),
        "duplicate-window"
    );
    assert!(
        !ProposeError::Refused(RefusalKind::DuplicateWindow)
            .message()
            .contains("win-1")
    );
    assert!(
        !format!(
            "{:?}",
            ProposeError::Refused(RefusalKind::CrossDomainMismatch)
        )
        .contains("SECRET")
    );
}

#[test]
fn acknowledge_divergence_clears_pending_desired() {
    use tiler_core::contract::AckOutcome;
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    let base = session.accepted_revision();
    session
        .propose(
            &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
            &obs,
            &correlation("corr-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("propose stages pending desired");
    assert!(session.has_pending());
    assert!(session.has_pending_desired());
    // Wrong correlation diverges terminally.
    let err = session
        .acknowledge(&AdapterAck::new(
            correlation("corr-9"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect_err("correlation mismatch must diverge");
    assert_eq!(
        err,
        tiler_core::reconcile::AckError::Diverged(DivergenceKind::CorrelationMismatch)
    );
    assert_eq!(
        session.divergence(),
        Some(DivergenceKind::CorrelationMismatch)
    );
    assert!(!session.has_pending());
    assert!(!session.has_pending_desired());
    // Topology unchanged: win-2 was never committed.
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    // Terminal: further proposals diverge.
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    assert_eq!(
        session.propose(
            &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
            &obs,
            &correlation("corr-3"),
            &LifecycleCapabilities::full(),
        ),
        Err(ProposeError::Diverged(DivergenceKind::CorrelationMismatch))
    );
    assert!(!session.has_pending_desired());
}

#[test]
fn verify_divergence_clears_pending_desired() {
    use tiler_core::contract::LifecyclePostObservation;
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    let obs = complete_observation(&session, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &admit_command("win-2", "out-1", "ws-1", placement(120, 80)),
            &obs,
            &correlation("corr-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("propose");
    session
        .acknowledge(&AdapterAck::new(
            correlation("corr-2"),
            owner(),
            generation(),
            base,
            tiler_core::contract::AckOutcome::Accepted,
        ))
        .expect("ack");
    assert!(session.has_pending_desired());
    // Mismatched verified preconditions diverge terminally.
    let mut bad_preconditions = plan.dispatch.preconditions.clone();
    bad_preconditions.pop();
    let bad_post = LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 999),
        correlation("corr-2"),
        true,
        bad_preconditions,
        plan.dispatch.operation.clone(),
    );
    let err = session
        .verify_lifecycle(&bad_post)
        .expect_err("precondition mismatch must diverge");
    assert_eq!(
        err,
        tiler_core::reconcile::VerifyError::Diverged(DivergenceKind::PostconditionMismatch)
    );
    assert_eq!(
        session.divergence(),
        Some(DivergenceKind::PostconditionMismatch)
    );
    assert!(!session.has_pending());
    assert!(!session.has_pending_desired());
    // No commit applied.
    assert_eq!(session.accepted_revision(), base);
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
}

#[test]
fn lifecycle_policy_version_bound_and_rejected_when_tampered() {
    use tiler_core::contract::{
        LifecycleIntent, LifecycleOperation, LifecyclePlan, LifecyclePostObservation,
    };
    use tiler_core::reconcile::Reconciler;
    // Session-issued plans carry the explicit cosmic_v1 binding.
    let mut session = single_domain_session();
    let plan = admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-1",
    );
    assert_eq!(plan.dispatch.policy_version, LIFECYCLE_POLICY_VERSION);
    // A hand-tampered lifecycle plan with a foreign policy version diverges
    // without dispatch; frozen movement planning is unaffected.
    let mut reconciler = Reconciler::new(owner(), generation(), 0, 7).expect("reconciler");
    let mut bad = LifecyclePlan::for_operation(
        LifecycleIntent::Admit {
            window: WindowId("win-9".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
        LifecycleOperation::AdmitDeferred {
            window: WindowId("win-9".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
    );
    bad.policy_version = 999;
    assert!(!bad.valid_policy());
    let obs = Observation::new(owner(), generation(), 0, 7);
    assert_eq!(
        reconciler.propose_lifecycle(
            &bad,
            &obs,
            &correlation("corr-9"),
            &LifecycleCapabilities::full(),
        ),
        Err(tiler_core::reconcile::ProposeError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    // Frozen R1-R4 movement surface still validates independently.
    assert_eq!(tiler_core::contract::POLICY_VERSION, 1);
    let _ = LifecyclePostObservation::new(
        obs,
        correlation("corr-9"),
        true,
        Vec::new(),
        bad.operation.clone(),
    );
}

#[test]
fn two_workspaces_on_same_output_are_isolated() {
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("out-1", "ws-a", 120, 80),
            domain("out-1", "ws-b", 120, 80),
        ],
    )
    .expect("same-output two-workspace session");
    // Deterministic domain order preserved with duplicate output ids.
    let snapshot = session.snapshot();
    assert_eq!(snapshot.domains.len(), 2);
    assert_eq!(snapshot.domains[0].output.0, "out-1");
    assert_eq!(snapshot.domains[0].workspace.0, "ws-a");
    assert_eq!(snapshot.domains[1].output.0, "out-1");
    assert_eq!(snapshot.domains[1].workspace.0, "ws-b");
    // Isolated admission per logical domain.
    admit_and_commit(
        &mut session,
        "win-a",
        "out-1",
        "ws-a",
        placement(120, 80),
        "corr-1",
    );
    let plan_b = admit_and_commit(
        &mut session,
        "win-b",
        "out-1",
        "ws-b",
        placement(120, 80),
        "corr-2",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-a"),
        vec!["leaf-win-a".to_string()]
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-b"),
        vec!["leaf-win-b".to_string()]
    );
    // Domain-scoped focus follows the last admission.
    assert_eq!(
        session.focus(),
        (
            Some(domain_key("out-1", "ws-b")),
            Some(NodeId("leaf-win-b".to_owned()))
        )
    );
    // Geometry is domain-scoped.
    assert_eq!(plan_b.desired_geometry.len(), 1);
    assert_eq!(plan_b.desired_geometry[0].workspace.0, "ws-b");
    assert_eq!(plan_b.desired_geometry[0].output.0, "out-1");
    // Per-domain directional snapshots stay unique and reciprocal.
    let snap_a = session
        .domain_snapshot(
            &OutputId("out-1".to_owned()),
            &WorkspaceId("ws-a".to_owned()),
        )
        .expect("domain snapshot a");
    assert_eq!(snap_a.outputs.len(), 1);
    assert_eq!(snap_a.windows.len(), 1);
    assert_eq!(snap_a.windows[0].workspace.0, "ws-a");
    let snap_b = session
        .domain_snapshot(
            &OutputId("out-1".to_owned()),
            &WorkspaceId("ws-b".to_owned()),
        )
        .expect("domain snapshot b");
    assert_eq!(snap_b.windows.len(), 1);
    assert_eq!(snap_b.windows[0].workspace.0, "ws-b");
    assert!(
        session
            .domain_snapshot(
                &OutputId("out-1".to_owned()),
                &WorkspaceId("ws-zzz".to_owned())
            )
            .is_none()
    );
    // Isolated removal: removing ws-a leaves ws-b untouched and preserves focus.
    remove_and_commit(&mut session, "win-a", "corr-3");
    assert!(leaves_of(&session, "out-1", "ws-a").is_empty());
    assert_eq!(
        leaves_of(&session, "out-1", "ws-b"),
        vec!["leaf-win-b".to_string()]
    );
    assert_eq!(
        session.focus(),
        (
            Some(domain_key("out-1", "ws-b")),
            Some(NodeId("leaf-win-b".to_owned()))
        )
    );
    // Removing the last window empties the second domain and clears focus.
    remove_and_commit(&mut session, "win-b", "corr-4");
    assert!(leaves_of(&session, "out-1", "ws-a").is_empty());
    assert!(leaves_of(&session, "out-1", "ws-b").is_empty());
    assert_eq!(session.focus(), (None, None));
    assert!(session.snapshot().windows.is_empty());
}

#[test]
fn deferred_removal_commits_without_topology_effect() {
    use tiler_core::contract::AckOutcome;
    let mut session = single_domain_session();
    // Defer a floating window, then admit a tiled window.
    let obs = complete_observation(&session, vec![exception_observed("win-9", "out-1", "ws-1")]);
    let deferred = SessionCommand::Admit {
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
        placement_bounds: placement(120, 80),
        suppress_fixed_float: false,
    };
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &deferred,
            &obs,
            &correlation("corr-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("deferred proposes");
    session
        .acknowledge(&AdapterAck::new(
            correlation("corr-1"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 211),
            correlation("corr-1"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
    assert!(session.is_exception(&WindowId("win-9".to_owned())));
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    let focus_before = session.focus();
    // Remove the deferred exception window: topology unchanged, set shrinks.
    let plan = remove_and_commit(&mut session, "win-9", "corr-3");
    assert!(plan.desired_geometry.is_empty());
    assert!(!session.is_exception(&WindowId("win-9".to_owned())));
    assert_eq!(session.exception_count(), 0);
    assert_eq!(
        leaves_of(&session, "out-1", "ws-1"),
        vec!["leaf-win-1".to_string()]
    );
    assert_eq!(session.focus(), focus_before);
    match &plan.dispatch.operation {
        tiler_core::contract::LifecycleOperation::RemoveDeferred {
            window,
            output,
            workspace,
        } => {
            assert_eq!(window.0, "win-9");
            assert_eq!(output.0, "out-1");
            assert_eq!(workspace.0, "ws-1");
        }
        other => panic!("expected deferred removal, got {other:?}"),
    }
}

#[test]
fn inconsistent_exception_behavior_is_malformed() {
    let mut session = single_domain_session();
    // Tiled admission carrying Some(Defer) with no flags is malformed.
    let obs = complete_observation(&session, vec![tiled_observed("win-1", "out-1", "ws-1")]);
    let inconsistent = SessionCommand::Admit {
        window: WindowId("win-1".to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: Some(ExceptionBehavior::Defer),
        placement_bounds: placement(120, 80),
        suppress_fixed_float: false,
    };
    assert_eq!(
        session
            .propose(
                &inconsistent,
                &obs,
                &correlation("corr-1"),
                &LifecycleCapabilities::full(),
            )
            .expect_err("inconsistent behavior must be malformed"),
        ProposeError::Refused(RefusalKind::MalformedInput)
    );
    assert_eq!(session.divergence(), None);
    assert!(!session.has_pending());
    assert!(!session.has_pending_desired());
    // Still usable.
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "corr-2",
    );
}

#[test]
fn portable_session_module_prohibits_platform_imports() {
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
                "session.rs import line {raw_line:?} contains banned {token:?}"
            );
        }
    }
    assert!(
        SESSION.contains("transport-independent") || SESSION.contains("transport-neutral"),
        "session.rs should document its transport-independent boundary"
    );
}

#[test]
fn uneven_proportional_insertion_and_removal_preserve_ratios() {
    use tiler_core::cosmic_v1::{proportional_insertion_shares, proportional_removal_shares};
    // Uneven survivors preserve ratios on admission: [389, 409] + entrant.
    let inserted = proportional_insertion_shares(&[389, 409], 2).expect("insert");
    assert_eq!(inserted, vec![389, 409, 399]);
    assert_eq!(inserted[0] * 409, inserted[1] * 389);
    // Removal redistributes proportionally before collapse: drop entrant.
    let removed = proportional_removal_shares(&inserted, 2).expect("remove");
    assert_eq!(removed, vec![389, 409]);
    // Session-level: automatic admission never uses N-ary same-axis append.
    // Admit three wide windows: the third wraps the focused leaf old/new in
    // an ordered binary group with the admission axis and [1, 1] shares.
    let mut session = single_domain_session();
    admit_and_commit(
        &mut session,
        "win-1",
        "out-1",
        "ws-1",
        placement(120, 80),
        "c-1",
    );
    admit_and_commit(
        &mut session,
        "win-2",
        "out-1",
        "ws-1",
        placement(120, 80),
        "c-2",
    );
    admit_and_commit(
        &mut session,
        "win-3",
        "out-1",
        "ws-1",
        placement(120, 80),
        "c-3",
    );
    let snapshot = session.snapshot();
    let tree = snapshot.domains[0].tree.clone().expect("tree");
    match tree {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            // Rejects flat [1, 1, 1] append: binary root plus focused wrapper.
            assert_eq!(axis, Axis::Horizontal);
            assert_eq!(shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected focused binary wrapper, got {other:?}"),
            }
        }
        other => panic!("expected group, got {other:?}"),
    }
}
