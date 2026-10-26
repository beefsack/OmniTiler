#![forbid(unsafe_code)]
//! R-INS-05 float-focus admission + R-CLOSE-02 fresh-reopen fixtures (0.1 triage D05/D07).
//!
//! Verification-only characterization of the shared Engine route in a
//! 120x80 test domain (gap 0):
//! - float-focus convergence: `converge_observation` with `focused_window = F`
//!   (ordinary float exception) admits C at the retained tiled focus B as
//!   nested `V[B,C]` (`H[A,V[B,C]]`; A keeps its 60x80 half, B/C take 60x40
//!   each); the float is preserved and the retained focus stays B until
//!   native observation follows C.
//! - genuine no-anchor admission still root-wraps when no tiled focus history
//!   exists at all.
//! - R-CLOSE-02: close focused B, explicitly refocus C, reopen fresh at C
//!   (`H[A,V[C,D]]`, newcomer focus), never an old-slot restore; survivor
//!   ratio rescale is not asserted here.

use std::collections::BTreeMap;

use crate::boundary::{ProjectionKind, project_retained_tiled_geometry};
use crate::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecyclePostObservation, Observation,
};
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, Session, SessionCommand,
    SessionObservation, SessionPlan,
};
use crate::size_hints::WindowSizeHints;

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

fn float_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
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

fn admit_with_placement(
    session: &mut Session,
    window: &str,
    output: &str,
    workspace: &str,
    placement_bounds: Rect,
    corr: &str,
) -> SessionPlan {
    let extra = vec![tiled_observed(window, output, workspace)];
    let obs = complete_observation(session, extra);
    let base = session.accepted_revision();
    let command = SessionCommand::Admit {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds,
        suppress_fixed_float: false,
    };
    let plan = session
        .propose(
            &command,
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("admit {window} proposes: {e:?}"));
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

fn remove_and_commit(session: &mut Session, window: &str, corr: &str) {
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
            AckOutcome::Accepted,
        ))
        .expect("ack");
    let post = LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 300 + base),
        correlation(corr),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    );
    session.verify_lifecycle(&post).expect("commit");
}

fn domain_key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

#[test]
fn float_focus_convergence_admits_newcomer_at_prior_tiled_focus() {
    // R-INS-05 retained-route characterization on the unchanged policy: the
    // native signal is `focused_window = F` (ordinary float exception)
    // through the actual Engine reconciliation primitive. Float focus never
    // takes tile focus, so C tiles at the retained tiled focus B under the
    // ordinary long-edge rule (B is 60x80 tall, so Vertical): `H[A,V[B,C]]`
    // with A keeping its half. The retained focus stays B, and F is
    // preserved. A later observation with native focus on C syncs focus to
    // the newcomer.
    let mut session = single_domain_session();
    let wide = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    admit_with_placement(&mut session, "win-a", "out-1", "ws-1", wide, "conv-a");
    admit_with_placement(&mut session, "win-b", "out-1", "ws-1", wide, "conv-b");

    // Float F appears with native focus on it.
    let counts = session
        .converge_observation(
            &complete_observation(&session, vec![float_observed("win-f", "out-1", "ws-1")]),
            Some(&WindowId("win-f".to_owned())),
        )
        .expect("float converges");
    assert_eq!(counts.flags_adopted, 1);
    assert!(session.is_exception(&WindowId("win-f".to_owned())));
    // Prior valid tiled focus B is retained: a float never takes leaf focus.
    assert_eq!(
        session.focus(),
        (
            Some(domain_key("out-1", "ws-1")),
            Some(NodeId("leaf-win-b".to_owned()))
        )
    );

    // Newcomer C opens while F holds native focus.
    let counts = session
        .converge_observation(
            &complete_observation(&session, vec![tiled_observed("win-c", "out-1", "ws-1")]),
            Some(&WindowId("win-f".to_owned())),
        )
        .expect("newcomer converges");
    assert_eq!(counts.admitted, 1);
    match &session.snapshot().domains[0].tree {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(
                *axis,
                Axis::Horizontal,
                "root stays H so A keeps its left half"
            );
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-a");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(
                        *axis,
                        Axis::Vertical,
                        "tall B (60x80) splits Vertical under long-edge admission"
                    );
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-b");
                    assert_eq!(children[1].id().0, "leaf-win-c");
                }
                other => panic!("expected nested V[B,C] beside A, got {other:?}"),
            }
        }
        other => panic!("expected H[A,V[B,C]], got {other:?}"),
    }
    // Concrete projected frames for the 120x80 test domain (gap 0) via the
    // shared retained-geometry read path: A keeps its 60x80 left half, B/C
    // split the right half 60x40 each.
    let retained = session.domains().first().expect("domain").clone();
    let (focus_domain, focus_leaf) = session.focus();
    let mut frames: Vec<(String, Rect)> = project_retained_tiled_geometry(
        &session,
        &domain_key("out-1", "ws-1"),
        retained.bounds,
        retained.gap,
        Some((
            focus_domain.expect("focus domain"),
            focus_leaf.expect("focus leaf"),
        )),
        ProjectionKind::Reconcile,
        &BTreeMap::<WindowId, WindowSizeHints>::new(),
        &[],
    )
    .expect("projects")
    .geometry
    .iter()
    .map(|g| (g.window.0.clone(), g.rect))
    .collect();
    frames.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        frames,
        vec![
            (
                "win-a".to_owned(),
                Rect {
                    x: 0,
                    y: 0,
                    w: 60,
                    h: 80
                }
            ),
            (
                "win-b".to_owned(),
                Rect {
                    x: 60,
                    y: 0,
                    w: 60,
                    h: 40
                }
            ),
            (
                "win-c".to_owned(),
                Rect {
                    x: 60,
                    y: 40,
                    w: 60,
                    h: 40
                }
            ),
        ],
        "A keeps its half; B/C stack vertically in the right half"
    );
    assert!(session.is_exception(&WindowId("win-f".to_owned())));
    assert_eq!(
        session.focus(),
        (
            Some(domain_key("out-1", "ws-1")),
            Some(NodeId("leaf-win-b".to_owned()))
        ),
        "float focus never displaces the retained tiled focus"
    );

    // Native focus follows the newcomer: focus syncs to C with no topology change.
    let counts = session
        .converge_observation(
            &complete_observation(&session, Vec::new()),
            Some(&WindowId("win-c".to_owned())),
        )
        .expect("focus follow converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(
        session.focus(),
        (
            Some(domain_key("out-1", "ws-1")),
            Some(NodeId("leaf-win-c".to_owned()))
        )
    );
}

#[test]
fn seeded_mru_admission_anchors_without_focus() {
    // No-focus boundary characterization under cursor-first placement: a
    // multi-leaf tree built with no tiled focus history at all (focusless
    // converges, so no evolving focus anywhere) anchors the next admission
    // at the seeded MRU leaf — the per-leg commit history the D8 seeding
    // preserves for focused-removal fallback — rather than root-wrapping.
    // Genuine per-domain focus history (remembered last-active) would win
    // over this seed; with none anywhere the seed is the last activity.
    let mut session = single_domain_session();
    for (window, extra) in [
        ("win-a", vec![tiled_observed("win-a", "out-1", "ws-1")]),
        ("win-b", vec![tiled_observed("win-b", "out-1", "ws-1")]),
    ] {
        let counts = session
            .converge_observation(&complete_observation(&session, extra), None)
            .unwrap_or_else(|e| panic!("converge {window}: {e:?}"));
        assert_eq!(counts.admitted, 1);
        assert_eq!(session.focus(), (None, None));
    }
    let counts = session
        .converge_observation(
            &complete_observation(&session, vec![tiled_observed("win-c", "out-1", "ws-1")]),
            None,
        )
        .expect("third converge admits");
    assert_eq!(counts.admitted, 1);
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
            assert_eq!(children[0].id().0, "leaf-win-a");
            match &children[1] {
                Node::Group { children, .. } => {
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-b");
                    assert_eq!(children[1].id().0, "leaf-win-c");
                }
                other => panic!("expected wrapped H[B,C] second, got {other:?}"),
            }
        }
        other => panic!("expected H[A,H[B,C]], got {other:?}"),
    }
    assert_eq!(session.focus(), (None, None));
}

#[test]
fn close_then_reopen_is_fresh_admission_at_refocus_not_old_slot() {
    // R-CLOSE-02 (D07): H[A,B,C] with B active; close focused B (survivors
    // keep order, MRU fallback would land on A here); explicitly refocus the
    // different tile C; reopen a same-app window. Fresh admission anchors at
    // C (`H[A,V[C,D]]`, newcomer focus), never an old-slot restore. Survivor
    // manual-ratio rescale is not asserted here.
    let mut session = single_domain_session();
    let wide = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    admit_with_placement(&mut session, "win-a", "out-1", "ws-1", wide, "close-a");
    admit_with_placement(&mut session, "win-b", "out-1", "ws-1", wide, "close-b");
    admit_with_placement(&mut session, "win-c", "out-1", "ws-1", wide, "close-c");
    let key = domain_key("out-1", "ws-1");
    // End state of the row Given: B active (stack tops B over A over C).
    assert!(session.sync_focus_from_window(&key, &WindowId("win-a".to_owned())));
    assert!(session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    assert_eq!(
        session.focus(),
        (Some(key.clone()), Some(NodeId("leaf-win-b".to_owned())))
    );

    // Close focused B: survivors keep order.
    remove_and_commit(&mut session, "win-b", "close-rm-b");
    let leaves: Vec<String> = match &session.snapshot().domains[0].tree {
        Some(Node::Group { children, .. }) => children.iter().map(|c| c.id().0.clone()).collect(),
        other => panic!("expected surviving H[A,C], got {other:?}"),
    };
    assert_eq!(
        leaves,
        vec!["leaf-win-a".to_string(), "leaf-win-c".to_string()]
    );

    // Explicitly refocus the different tile C (the row's "focus C" step).
    assert!(session.sync_focus_from_window(&key, &WindowId("win-c".to_owned())));
    assert_eq!(
        session.focus(),
        (Some(key.clone()), Some(NodeId("leaf-win-c".to_owned())))
    );

    // Reopen a same-app window under a new id: fresh admission at C.
    let plan = admit_with_placement(
        &mut session,
        "win-b2",
        "out-1",
        "ws-1",
        Rect {
            x: 60,
            y: 0,
            w: 60,
            h: 80,
        },
        "close-reopen",
    );
    match &session.snapshot().domains[0].tree {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-a");
            match &children[1] {
                Node::Group { axis, children, .. } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(children.len(), 2);
                    // Fresh at focused C: C first, newcomer after. An old-slot
                    // restore would order the reopened window before C.
                    assert_eq!(children[0].id().0, "leaf-win-c");
                    assert_eq!(children[1].id().0, "leaf-win-b2");
                }
                other => panic!("expected fresh V[C,D] beside A, got {other:?}"),
            }
        }
        other => panic!("expected H[A,V[C,D]], got {other:?}"),
    }
    let (_, focus) = session.focus();
    assert_eq!(focus, Some(NodeId("leaf-win-b2".to_owned())));
    assert_eq!(plan.desired_geometry.len(), 3);
}

/// First layout leaf window for one domain, in deterministic child order
/// (tentative no-history tie-break for view returns).
fn first_leaf_window(session: &Session, key: &DomainKey) -> Option<WindowId> {
    let mut node = session.tree_for(key)?;
    loop {
        match node {
            Node::Leaf { id } => {
                return session
                    .snapshot()
                    .windows
                    .iter()
                    .find(|link| &link.leaf == id)
                    .map(|link| link.window.clone());
            }
            Node::Group { children, .. } => {
                node = children.first()?;
            }
        }
    }
}

#[test]
fn view_return_candidates_prefers_remembered_then_mru_then_first() {
    // Shared-core view-return chain: a fresh converge with no observed
    // focus records no remembered leaf while the MRU stack seeds in
    // admission order, so the first candidate is the MRU top (win-b), not
    // the first layout leaf (win-a). A remembered leaf wins over a newer
    // MRU top, ineligible top candidates fall through per candidate at
    // the call site, and an empty domain resolves to no candidate.
    fn first_candidate(session: &Session, key: &DomainKey) -> Option<NodeId> {
        session.view_return_candidates(key).into_iter().next()
    }
    let mut session = single_domain_session();
    let key = domain_key("out-1", "ws-1");
    let obs = complete_observation(
        &session,
        vec![
            tiled_observed("win-a", "out-1", "ws-1"),
            tiled_observed("win-b", "out-1", "ws-1"),
        ],
    );
    session.converge_observation(&obs, None).expect("converge");
    assert_eq!(session.focus(), (None, None));
    assert_eq!(
        first_leaf_window(&session, &key)
            .as_ref()
            .map(|w| w.0.as_str()),
        Some("win-a")
    );
    assert_eq!(
        first_candidate(&session, &key),
        Some(NodeId("leaf-win-b".to_owned())),
        "MRU top wins with no remembered leaf"
    );
    // Ordinary activation advances the MRU stack only; the remembered
    // leaf still wins the return.
    assert!(session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    assert!(session.remember_focus(&key, &WindowId("win-a".to_owned())));
    assert!(session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    assert_eq!(
        first_candidate(&session, &key),
        Some(NodeId("leaf-win-a".to_owned())),
        "remembered leaf beats the newer MRU top"
    );
    // Candidate order documents the fall-through: remembered first, then
    // MRU recency, then layout order, deduplicated.
    assert_eq!(
        session.view_return_candidates(&key),
        vec![
            NodeId("leaf-win-a".to_owned()),
            NodeId("leaf-win-b".to_owned())
        ]
    );
    // Empty domain: no candidate, never invented focus.
    let empty = single_domain_session();
    assert_eq!(first_candidate(&empty, &key), None);
}
