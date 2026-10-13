#![forbid(unsafe_code)]
//! Parent split-axis toggle integration tests (R-LAY-01, portable, headless).
//!
//! Drives the real `session::Session` API through full
//! propose/acknowledge/verify cycles. No live compositor state.

use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecyclePostObservation, Observation,
};
use tiler_core::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionObservation, SessionPlan,
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

fn wide_session() -> Session {
    Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![OutputDomain {
            id: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1200,
                h: 800,
            },
            gap: 0,
            adjacent: std::collections::BTreeMap::new(),
        }],
    )
    .expect("session")
}

fn tiled_observed(window: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

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

fn commit(
    session: &mut Session,
    command: &SessionCommand,
    observation: &SessionObservation,
    corr: &str,
) -> SessionPlan {
    let base = session.accepted_revision();
    let plan = session
        .propose(
            command,
            observation,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("propose {command:?}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    session.verify_lifecycle(&post).expect("commit");
    plan
}

fn admit_and_commit(session: &mut Session, window: &str, placement: Rect, corr: &str) {
    let obs = complete_observation(session, vec![tiled_observed(window)]);
    commit(
        session,
        &SessionCommand::Admit {
            window: WindowId(window.to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            exceptions: ExceptionFlags::none(),
            exception_behavior: None,
            placement_bounds: placement,
            suppress_fixed_float: false,
        },
        &obs,
        corr,
    );
}

fn toggle_and_commit(session: &mut Session, window: &str, corr: &str) -> SessionPlan {
    let obs = complete_observation(session, Vec::new());
    commit(
        session,
        &SessionCommand::ToggleOrientation {
            window: WindowId(window.to_owned()),
        },
        &obs,
        corr,
    )
}

fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

fn root_axis(session: &Session) -> Axis {
    match session.tree_for(&key()).expect("tree") {
        Node::Group { axis, .. } => *axis,
        Node::Leaf { .. } => panic!("expected a root group"),
    }
}

fn root_children(session: &Session) -> (Axis, Vec<NodeId>, Vec<u64>) {
    match session.tree_for(&key()).expect("tree").clone() {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => (
            axis,
            children.iter().map(|child| child.id().clone()).collect(),
            shares,
        ),
        Node::Leaf { .. } => panic!("expected a root group"),
    }
}

fn pair_session() -> Session {
    let mut session = wide_session();
    admit_and_commit(
        &mut session,
        "win-a",
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
        "corr-a",
    );
    // Wide placement selects the Horizontal root (long-edge rule).
    admit_and_commit(
        &mut session,
        "win-b",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        "corr-b",
    );
    assert_eq!(root_axis(&session), Axis::Horizontal);
    session
}

#[test]
fn root_horizontal_flips_to_vertical() {
    let mut session = pair_session();
    let (focus_domain, focus_leaf) = session.focus();
    assert_eq!(focus_leaf.as_ref().expect("focus").0, "leaf-win-b");
    let plan = toggle_and_commit(&mut session, "win-b", "corr-t1");
    assert_eq!(root_axis(&session), Axis::Vertical);
    // Child order, shares, and focus are preserved; geometry is restacked.
    let (axis, children, shares) = root_children(&session);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(
        children.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        vec!["leaf-win-a", "leaf-win-b"]
    );
    assert_eq!(shares, vec![1, 1]);
    assert_eq!(
        (session.focus().0, session.focus().1),
        (focus_domain, focus_leaf)
    );
    assert_eq!(plan.desired_geometry.len(), 2);
    for entry in &plan.desired_geometry {
        assert_eq!(entry.rect.w, 1200, "vertical split keeps full width");
        assert_eq!(entry.rect.h, 400, "vertical split stacks halves");
    }
    assert!(
        !plan
            .desired_geometry
            .iter()
            .any(|entry| entry.overconstrained)
    );
}

#[test]
fn double_toggle_restores_root() {
    let mut session = pair_session();
    let before = session.tree_for(&key()).expect("tree").clone();
    toggle_and_commit(&mut session, "win-b", "corr-t1");
    assert_eq!(root_axis(&session), Axis::Vertical);
    let plan = toggle_and_commit(&mut session, "win-b", "corr-t2");
    assert_eq!(session.tree_for(&key()).expect("tree"), &before);
    // Geometry returns to the side-by-side allocation.
    assert_eq!(plan.desired_geometry.len(), 2);
    for entry in &plan.desired_geometry {
        assert_eq!(entry.rect.w, 600, "horizontal split halves the width");
        assert_eq!(entry.rect.h, 800, "horizontal split keeps full height");
    }
}

#[test]
fn nested_toggle_flips_only_the_immediate_parent() {
    use tiler_core::directional::WindowLink;
    // R-LAY-05 shape with unequal shares: H[A, V[B*, C]], focus on B.
    let before = Node::Group {
        id: NodeId::from("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId::from("a"),
            },
            Node::Group {
                id: NodeId::from("g"),
                axis: Axis::Vertical,
                children: vec![
                    Node::Leaf {
                        id: NodeId::from("b"),
                    },
                    Node::Leaf {
                        id: NodeId::from("c"),
                    },
                ],
                shares: vec![1, 3],
            },
        ],
        shares: vec![2, 1],
    };
    let mut session = wide_session();
    let links = ["win-a", "win-b", "win-c"]
        .iter()
        .zip(["a", "b", "c"].iter())
        .map(|(window, leaf)| WindowLink {
            window: WindowId((*window).to_owned()),
            leaf: NodeId::from(*leaf),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        })
        .collect::<Vec<_>>();
    let observation = SessionObservation {
        observation: Observation::new(owner(), generation(), 0, 0),
        windows: ["win-a", "win-b", "win-c"]
            .iter()
            .map(|name| ObservedWindow {
                window: WindowId((*name).to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
            })
            .collect(),
    };
    let commit_corr = correlation("corr-fit");
    let plan = session
        .propose_fitted_admit(
            before.clone(),
            links,
            NodeId::from("b"),
            &WindowId("win-b".to_owned()),
            &OutputId("out-1".to_owned()),
            &WorkspaceId("ws-1".to_owned()),
            &observation,
            &commit_corr,
            &LifecycleCapabilities::full(),
        )
        .expect("fit");
    let base = plan.dispatch.base_revision;
    session
        .acknowledge(&AdapterAck::new(
            commit_corr.clone(),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 0),
            commit_corr,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "b");

    // First toggle flips only the immediate (nested) parent to H.
    let plan = toggle_and_commit(&mut session, "win-b", "corr-t1");
    assert_eq!(plan.desired_geometry.len(), 3);
    let after_first = session.tree_for(&key()).expect("tree").clone();
    match &after_first {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            assert_eq!(*axis, Axis::Horizontal, "root axis untouched");
            assert_eq!(*shares, vec![2, 1], "root shares untouched");
            assert_eq!(children[0].id().0, "a", "root order untouched");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal, "nested parent flips V->H");
                    assert_eq!(*shares, vec![1, 3], "nested shares untouched");
                    assert_eq!(
                        children
                            .iter()
                            .map(|child| child.id().0.as_str())
                            .collect::<Vec<_>>(),
                        vec!["b", "c"],
                        "nested order untouched"
                    );
                }
                Node::Leaf { .. } => panic!("expected the nested group"),
            }
        }
        Node::Leaf { .. } => panic!("expected the root group"),
    }
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "b");

    // Second toggle restores the exact original tree.
    toggle_and_commit(&mut session, "win-b", "corr-t2");
    assert_eq!(session.tree_for(&key()).expect("tree"), &before);
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "b");
}

#[test]
fn unequal_shares_order_and_focus_survive_the_flip() {
    use tiler_core::directional::WindowLink;
    let mut session = wide_session();
    let tree = Node::Group {
        id: NodeId::from("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId::from("a"),
            },
            Node::Leaf {
                id: NodeId::from("b"),
            },
            Node::Leaf {
                id: NodeId::from("c"),
            },
        ],
        shares: vec![1, 2, 3],
    };
    let links = ["win-a", "win-b", "win-c"]
        .iter()
        .zip(["a", "b", "c"].iter())
        .map(|(window, leaf)| WindowLink {
            window: WindowId((*window).to_owned()),
            leaf: NodeId::from(*leaf),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        })
        .collect::<Vec<_>>();
    let observation = SessionObservation {
        observation: Observation::new(owner(), generation(), 0, 0),
        windows: ["win-a", "win-b", "win-c"]
            .iter()
            .map(|name| ObservedWindow {
                window: WindowId((*name).to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
            })
            .collect(),
    };
    let commit_corr = correlation("corr-fit");
    let plan = session
        .propose_fitted_admit(
            tree,
            links,
            NodeId::from("b"),
            &WindowId("win-b".to_owned()),
            &OutputId("out-1".to_owned()),
            &WorkspaceId("ws-1".to_owned()),
            &observation,
            &commit_corr,
            &LifecycleCapabilities::full(),
        )
        .expect("fit");
    let base = plan.dispatch.base_revision;
    session
        .acknowledge(&AdapterAck::new(
            commit_corr.clone(),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 0),
            commit_corr,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "b");
    toggle_and_commit(&mut session, "win-b", "corr-t1");
    let (axis, children, shares) = root_children(&session);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(
        children.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(shares, vec![1, 2, 3], "shares travel with the flip");
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "b");
    // A second toggle restores the exact original tree, shares included.
    toggle_and_commit(&mut session, "win-b", "corr-t2");
    match session.tree_for(&key()).expect("tree") {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 2, 3]);
            assert_eq!(
                children
                    .iter()
                    .map(|child| child.id().0.as_str())
                    .collect::<Vec<_>>(),
                vec!["a", "b", "c"]
            );
        }
        Node::Leaf { .. } => panic!("expected the root group"),
    }
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "b");
}

#[test]
fn minima_reproject_through_the_existing_hints_path() {
    let mut session = pair_session();
    // Satisfiable but reallocating minima: stacked 1200x400 halves cannot
    // cover a 500 minimum height, so the projector takes slack from the
    // sibling (300/500), exactly like every other workflow. Shares stay
    // proportional in the retained tree; only the projection adjusts.
    let mut windows: Vec<ObservedWindow> = vec![tiled_observed("win-a"), tiled_observed("win-b")];
    windows[0].hints = WindowSizeHints {
        min_w: Some(200),
        min_h: Some(200),
        max_w: None,
        max_h: None,
    };
    windows[1].hints = WindowSizeHints {
        min_w: Some(800),
        min_h: Some(500),
        max_w: None,
        max_h: None,
    };
    let obs = SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            session.accepted_revision(),
            100 + session.accepted_revision(),
        ),
        windows,
    };
    let plan = commit(
        &mut session,
        &SessionCommand::ToggleOrientation {
            window: WindowId("win-b".to_owned()),
        },
        &obs,
        "corr-hints",
    );
    assert_eq!(root_axis(&session), Axis::Vertical);
    let rect_of = |plan: &SessionPlan, window: &str| {
        plan.desired_geometry
            .iter()
            .find(|entry| entry.window.0 == window)
            .unwrap_or_else(|| panic!("geometry for {window}"))
            .rect
    };
    let rect_a = rect_of(&plan, "win-a");
    let rect_b = rect_of(&plan, "win-b");
    assert!(
        !plan
            .desired_geometry
            .iter()
            .any(|entry| entry.overconstrained)
    );
    assert_eq!(
        rect_a,
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 300
        }
    );
    assert_eq!(
        rect_b,
        Rect {
            x: 0,
            y: 300,
            w: 1200,
            h: 500
        }
    );
    let (_, _, shares) = root_children(&session);
    assert_eq!(shares, vec![1, 1], "projection never rewrites shares");
    // Unsatisfiable minima keep the proportional fallback flagged
    // overconstrained, exactly like every other workflow. A fresh pair
    // toggles H->V (stacked 1200x400 halves) against a 700 minimum height.
    let mut tight_session = pair_session();
    let mut tight: Vec<ObservedWindow> = vec![tiled_observed("win-a"), tiled_observed("win-b")];
    // 300 + 700 exceeds the 800 stacked extent: unsatisfiable after the flip.
    tight[0].hints = WindowSizeHints {
        min_w: None,
        min_h: Some(300),
        max_w: None,
        max_h: None,
    };
    tight[1].hints = WindowSizeHints {
        min_w: None,
        min_h: Some(700),
        max_w: None,
        max_h: None,
    };
    let obs = SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            tight_session.accepted_revision(),
            100 + tight_session.accepted_revision(),
        ),
        windows: tight,
    };
    let plan = commit(
        &mut tight_session,
        &SessionCommand::ToggleOrientation {
            window: WindowId("win-b".to_owned()),
        },
        &obs,
        "corr-tight",
    );
    let entry = plan
        .desired_geometry
        .iter()
        .find(|entry| entry.window.0 == "win-b")
        .expect("mover geometry");
    assert!(
        entry.overconstrained,
        "unsatisfiable minimum flags the window"
    );
}

#[test]
fn lone_root_leaf_is_an_unchanged_noop() {
    let mut session = wide_session();
    admit_and_commit(
        &mut session,
        "win-a",
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
        "corr-a",
    );
    assert!(matches!(
        session.tree_for(&key()).expect("tree"),
        Node::Leaf { .. }
    ));
    let obs = complete_observation(&session, Vec::new());
    let error = session
        .propose(
            &SessionCommand::ToggleOrientation {
                window: WindowId("win-a".to_owned()),
            },
            &obs,
            &correlation("corr-t1"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("lone root leaf must refuse");
    assert_eq!(error, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!session.has_pending(), "no pending is staged");
    // No orientation hint is saved by the refusal: admitting B on the wide
    // fixture keeps the long-edge rule (Horizontal root, R-LAY-06 shape),
    // with order and the focused newcomer exactly as ordinary admission.
    admit_and_commit(
        &mut session,
        "win-b",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        "corr-b",
    );
    let (axis, children, _) = root_children(&session);
    assert_eq!(
        axis,
        Axis::Horizontal,
        "long-edge admission, no toggle hint"
    );
    assert_eq!(
        children.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        vec!["leaf-win-a", "leaf-win-b"]
    );
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "leaf-win-b");
}

#[test]
fn floating_subject_and_focus_mismatch_refuse() {
    let mut session = pair_session();
    // Float the focused window: it becomes an exception and focus falls back.
    let obs = complete_observation(&session, Vec::new());
    commit(
        &mut session,
        &SessionCommand::ToggleFloat {
            window: WindowId("win-b".to_owned()),
            float_geometry: Some(Rect {
                x: 240,
                y: 160,
                w: 720,
                h: 480,
            }),
        },
        &obs,
        "corr-f1",
    );
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "leaf-win-a");
    let obs = complete_observation(&session, Vec::new());
    let error = session
        .propose(
            &SessionCommand::ToggleOrientation {
                window: WindowId("win-b".to_owned()),
            },
            &obs,
            &correlation("corr-t1"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("floating subject must refuse");
    assert_eq!(error, ProposeError::Refused(RefusalKind::NotTiled));
    assert!(!session.has_pending(), "no pending is staged");

    // Floating focus with no tiled focus left: a lone window floated away.
    let mut single = wide_session();
    admit_and_commit(
        &mut single,
        "win-a",
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
        "corr-a",
    );
    let obs = complete_observation(&single, Vec::new());
    commit(
        &mut single,
        &SessionCommand::ToggleFloat {
            window: WindowId("win-a".to_owned()),
            float_geometry: None,
        },
        &obs,
        "corr-f2",
    );
    assert_eq!(single.focus(), (None, None));
    let obs = complete_observation(&single, Vec::new());
    let error = single
        .propose(
            &SessionCommand::ToggleOrientation {
                window: WindowId("win-a".to_owned()),
            },
            &obs,
            &correlation("corr-t2"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("floating focus must refuse");
    assert_eq!(error, ProposeError::Refused(RefusalKind::NotTiled));

    // A tiled but unfocused window mismatches focus; unknown windows refuse.
    let mut fresh = pair_session();
    let obs = complete_observation(&fresh, Vec::new());
    let error = fresh
        .propose(
            &SessionCommand::ToggleOrientation {
                window: WindowId("win-a".to_owned()),
            },
            &obs,
            &correlation("corr-t3"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("unfocused window must refuse");
    assert_eq!(error, ProposeError::Refused(RefusalKind::FocusMismatch));
    let error = fresh
        .propose(
            &SessionCommand::ToggleOrientation {
                window: WindowId("win-zzz".to_owned()),
            },
            &obs,
            &correlation("corr-t4"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("unknown window must refuse");
    assert_eq!(error, ProposeError::Refused(RefusalKind::UnknownWindow));
}

#[test]
fn overlay_flagged_focus_never_reaches_proposal() {
    // Fullscreen/maximized overlays refuse in the adapter before dispatch,
    // and only when focused (`windowIsFullscreen`/`windowIsMaximized` gate
    // on the focused id; sibling overlays ride as fit_excluded tiles and are
    // skipped only at native writes). A stale flagged observation fails core
    // completeness with no mutation, never the toggle itself.
    let mut session = pair_session();
    let before = session.tree_for(&key()).expect("tree").clone();
    let mut windows = vec![tiled_observed("win-a"), tiled_observed("win-b")];
    windows[1].fullscreen = true;
    let obs = SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            session.accepted_revision(),
            100 + session.accepted_revision(),
        ),
        windows,
    };
    let error = session
        .propose(
            &SessionCommand::ToggleOrientation {
                window: WindowId("win-b".to_owned()),
            },
            &obs,
            &correlation("corr-t1"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("flagged observation must refuse");
    assert_eq!(
        error,
        ProposeError::Refused(RefusalKind::PartialObservation)
    );
    assert!(!session.has_pending(), "no pending is staged");
    assert_eq!(
        session.tree_for(&key()).expect("tree"),
        &before,
        "refusal mutates nothing"
    );
}

#[test]
fn subsequent_admission_keeps_the_long_edge_rule() {
    let mut session = pair_session();
    toggle_and_commit(&mut session, "win-b", "corr-t1");
    assert_eq!(root_axis(&session), Axis::Vertical);
    // The focused leaf now projects wide (1200x400): admission wraps it in
    // a Horizontal group while the toggled root stays Vertical. No toggle
    // hint is retained anywhere in the admission path.
    admit_and_commit(
        &mut session,
        "win-c",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 400,
        },
        "corr-c",
    );
    let (axis, children, _) = root_children(&session);
    assert_eq!(axis, Axis::Vertical, "toggled root keeps its axis");
    assert_eq!(children.len(), 2);
    let nested = match session.tree_for(&key()).expect("tree") {
        Node::Group { children, .. } => children[1].clone(),
        Node::Leaf { .. } => panic!("expected a root group"),
    };
    match nested {
        Node::Group { axis, children, .. } => {
            assert_eq!(axis, Axis::Horizontal, "long-edge admission wraps wide");
            assert_eq!(
                children
                    .iter()
                    .map(|child| child.id().0.as_str())
                    .collect::<Vec<_>>(),
                vec!["leaf-win-b", "leaf-win-c"]
            );
        }
        Node::Leaf { .. } => panic!("expected an admission group"),
    }
    assert_eq!(session.focus().1.as_ref().expect("focus").0, "leaf-win-c");
}
