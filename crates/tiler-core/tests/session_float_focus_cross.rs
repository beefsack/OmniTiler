#![forbid(unsafe_code)]
//! Float-origin cross-output directional focus.
//!
//! Offline Rust authority tests for the adapter's local float-search miss
//! path (`Left`/`Right` only): after the local same-domain float/sticky
//! search finds no target, the shared reciprocal-adjacency remembered-tiled
//! fallback applies through an explicit float subject. Tile-origin behavior
//! (including the unflagged refusal of exception subjects) is unchanged.

use tiler_core::contract::{
    AckOutcome, AdapterAck, FocusCapabilities, FocusPostObservation, LifecycleCapabilities,
    LifecyclePostObservation, Observation,
};
use tiler_core::directional::{Direction, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionObservation,
};
use tiler_core::size_hints::WindowSizeHints;

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
fn domain_ws(output: &str, workspace: &str, adjacent: Vec<(Direction, &str)>) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        },
        gap: 4,
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
            domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")]),
            domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")]),
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
            hints: WindowSizeHints::none(),
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
            hints: WindowSizeHints::none(),
        })
        .collect();
    observed.extend(session.exception_observed());
    observed.push(ObservedWindow {
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
                window: WindowId(window.to_owned()),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
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
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, base),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
}
fn float_window(session: &mut Session, window: &str, corr: &str) {
    let obs = complete_obs(session);
    let base = session.accepted_revision();
    let correlation = correlation(corr);
    let plan = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId(window.to_owned()),
                float_geometry: None,
            },
            &obs,
            &correlation,
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("toggle-float {window} proposes: {e:?}"));
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
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, base),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
}
#[test]
fn float_cross_plans_remembered_target_and_commits_without_mutation() {
    let mut s = two_output_session();
    admit(&mut s, "win-a", "out-1", "ws-a", "ff-seed-1");
    admit(&mut s, "win-f", "out-1", "ws-a", "ff-seed-2");
    admit(&mut s, "win-x", "out-2", "ws-b", "ff-seed-3");
    float_window(&mut s, "win-f", "ff-float-1");
    let k1 = key("out-1", "ws-a");
    let obs = complete_obs(&s);
    let before = s.snapshot();
    let plan = s
        .propose_float_cross_output_focus(
            &k1,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &obs,
            &correlation("ff-cross"),
            &FocusCapabilities::full(),
        )
        .expect("float cross plans");
    assert_eq!(plan.desired_focus_domain, key("out-2", "ws-b"));
    assert_eq!(plan.dispatch.operation.from_window.0, "win-f");
    // Honestly leafless: no tile leaf is borrowed for the float subject, and
    // the floating-subject token replaces the leaf-occupancy claim.
    assert_eq!(plan.dispatch.operation.from_leaf, None);
    assert_eq!(
        plan.dispatch.preconditions[0],
        tiler_core::contract::FocusPrecondition::FocusedFloatingWindow
    );
    assert_eq!(plan.dispatch.operation.to_window.0, "win-x");
    assert_eq!(
        plan.dispatch.operation.route,
        vec![plan.desired_focus_leaf.clone()]
    );
    // No float geometry is fabricated: every desired entry is a tile.
    for entry in &plan.desired_geometry {
        assert_ne!(entry.window.0, "win-f");
    }
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
    assert_eq!(s.focus().0, Some(key("out-2", "ws-b")));
    let after = s.snapshot();
    assert_eq!(before.domains, after.domains);
    assert_eq!(before.windows, after.windows);
}

#[test]
fn float_cross_needs_no_source_tile() {
    // A float-only source domain crosses the same way: the operation stays
    // leafless and targets the adjacent output's remembered tile.
    let mut s = two_output_session();
    admit(&mut s, "win-f", "out-1", "ws-a", "fo-seed-1");
    admit(&mut s, "win-x", "out-2", "ws-b", "fo-seed-2");
    float_window(&mut s, "win-f", "fo-float-1");
    assert!(s.snapshot().windows.iter().all(|l| l.window.0 != "win-f"));
    let k1 = key("out-1", "ws-a");
    let obs = complete_obs(&s);
    let plan = s
        .propose_float_cross_output_focus(
            &k1,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &obs,
            &correlation("fo-cross"),
            &FocusCapabilities::full(),
        )
        .expect("float-only cross plans");
    assert_eq!(plan.desired_focus_domain, key("out-2", "ws-b"));
    assert_eq!(plan.dispatch.operation.from_window.0, "win-f");
    assert_eq!(plan.dispatch.operation.from_leaf, None);
    assert_eq!(plan.dispatch.operation.to_window.0, "win-x");
    assert_eq!(
        plan.dispatch.preconditions,
        vec![
            tiler_core::contract::FocusPrecondition::FocusedFloatingWindow,
            tiler_core::contract::FocusPrecondition::TargetLeafOccupied,
            tiler_core::contract::FocusPrecondition::FocusTargetsAdjacentOutput,
            tiler_core::contract::FocusPrecondition::AdapterMustVerifyPostconditions,
        ]
    );
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        correlation("fo-cross"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_focus(&FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 905),
        correlation("fo-cross"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify cross");
    assert_eq!(s.focus().0, Some(key("out-2", "ws-b")));
}

#[test]
fn float_cross_subject_eligibility_refuses_fail_closed() {
    let mut s = two_output_session();
    admit(&mut s, "win-a", "out-1", "ws-a", "fe-seed-1");
    admit(&mut s, "win-f", "out-1", "ws-a", "fe-seed-2");
    admit(&mut s, "win-x", "out-2", "ws-b", "fe-seed-3");
    float_window(&mut s, "win-f", "fe-float-1");
    let k1 = key("out-1", "ws-a");
    let obs = complete_obs(&s);
    // Unflagged cross keeps refusing exception subjects: tile-origin path unchanged.
    assert_eq!(
        s.propose_cross_output_focus(
            &k1,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &obs,
            &correlation("fe-unflagged"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::NotTiled))
    );
    assert!(!s.has_pending());
    // A tiled subject carrying the float flag refuses as a mismatch.
    assert_eq!(
        s.propose_float_cross_output_focus(
            &k1,
            &WindowId("win-a".to_owned()),
            Direction::Right,
            &obs,
            &correlation("fe-tiled"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::FocusMismatch))
    );
    assert!(!s.has_pending());
    // Unknown subjects refuse before any state.
    assert_eq!(
        s.propose_float_cross_output_focus(
            &k1,
            &WindowId("win-ghost".to_owned()),
            Direction::Right,
            &obs,
            &correlation("fe-unknown"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::UnknownWindow))
    );
    assert!(!s.has_pending());
    // A subject observation disagreeing with the known exception flags is a
    // partial observation (the outer completeness fence fires first).
    let mut unflagged_obs = complete_obs(&s);
    for entry in &mut unflagged_obs.windows {
        if entry.window.0 == "win-f" {
            entry.floating = false;
        }
    }
    assert_eq!(
        s.propose_float_cross_output_focus(
            &k1,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &unflagged_obs,
            &correlation("fe-observed-tiled"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::PartialObservation))
    );
    assert!(!s.has_pending());
}

#[test]
fn float_cross_edge_conditions_refuse_without_pending() {
    let mut s = two_output_session();
    admit(&mut s, "win-a", "out-1", "ws-a", "fx-seed-1");
    admit(&mut s, "win-f", "out-1", "ws-a", "fx-seed-2");
    admit(&mut s, "win-x", "out-2", "ws-b", "fx-seed-3");
    float_window(&mut s, "win-f", "fx-float-1");
    let k1 = key("out-1", "ws-a");
    let obs = complete_obs(&s);
    // Up/Down never cross from floats either.
    for (direction, corr) in [(Direction::Up, "fx-up"), (Direction::Down, "fx-down")] {
        assert_eq!(
            s.propose_float_cross_output_focus(
                &k1,
                &WindowId("win-f".to_owned()),
                direction,
                &obs,
                &correlation(corr),
                &FocusCapabilities::full()
            ),
            Err(ProposeError::Refused(RefusalKind::Unchanged))
        );
        assert!(!s.has_pending());
    }
    // Missing adjacency refuses without crossing.
    let mut lone = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain_ws("out-1", "ws-a", vec![])],
    )
    .expect("single session");
    admit(&mut lone, "win-a", "out-1", "ws-a", "fx-lone-1");
    admit(&mut lone, "win-f", "out-1", "ws-a", "fx-lone-2");
    float_window(&mut lone, "win-f", "fx-lone-float");
    let lone_obs = complete_obs(&lone);
    assert_eq!(
        lone.propose_float_cross_output_focus(
            &key("out-1", "ws-a"),
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &lone_obs,
            &correlation("fx-no-adjacent"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!lone.has_pending());
    // An empty target output refuses without crossing.
    let mut empty_target = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")]),
            domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")]),
        ],
    )
    .expect("empty-target session");
    admit(&mut empty_target, "win-a", "out-1", "ws-a", "fx-empty-1");
    admit(&mut empty_target, "win-f", "out-1", "ws-a", "fx-empty-2");
    float_window(&mut empty_target, "win-f", "fx-empty-float");
    let empty_obs = complete_obs(&empty_target);
    assert_eq!(
        empty_target.propose_float_cross_output_focus(
            &key("out-1", "ws-a"),
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &empty_obs,
            &correlation("fx-empty"),
            &FocusCapabilities::full()
        ),
        Err(ProposeError::Refused(RefusalKind::Unchanged))
    );
    assert!(!empty_target.has_pending());
}
