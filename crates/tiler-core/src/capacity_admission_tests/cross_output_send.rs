//! Cross-output send and R4 exhaustion: refusal preservation, overlap admit, no D2 redirect, withdraw-before-ack, focus/resize, drag agreement.
//!
//! Shared helpers live in the parent (`capacity_admission.rs`); this module
//! holds only `#[test]` fns.

use super::*;

#[test]
fn send_refused_when_target_unrepresentable_source_preserved() {
    // Source ws-1 roomy with a, b (b focused); the named target domain
    // does not exist. The send refuses UnknownDomain with the source
    // exactly preserved: same snapshot, same focus, no pending, mover
    // still tiled at home. (A merely exhausted destination no longer
    // refuses under the default float result: D3 capacity-floats there.)
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("o", "ws-1", 120, 80, 0),
            domain("o", "ws-2", 1, 1, 0),
        ],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "o", "ws-1", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "b", "o", "ws-1", placement(120, 40), "c-2");
    admit_and_commit(&mut session, "d", "o", "ws-2", placement(1, 1), "c-3");
    let seq_b = session.admission_sequence(&WindowId("b".to_owned()));
    // Focus b in ws-1 for the focused-mover gate.
    let key1 = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    assert!(session.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let (snapshot, focus, revision) = (
        session.snapshot(),
        session.focus(),
        session.accepted_revision(),
    );
    let obs = complete_observation(&session, Vec::new());
    let err = session
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o".to_owned()),
                target_workspace: WorkspaceId("ws-9".to_owned()),
                follow: true,
            },
            &obs,
            &correlation("s-1"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("unrepresentable target refuses");
    assert_eq!(
        err,
        crate::session::ProposeError::Refused(crate::session::RefusalKind::UnknownDomain),
        "honest refuse for an unrepresentable target"
    );
    assert_eq!(session.snapshot(), snapshot, "source preserved");
    assert_eq!(session.focus(), focus, "focus preserved");
    assert_eq!(session.accepted_revision(), revision);
    assert!(!session.has_pending() && !session.has_pending_desired());
    assert_eq!(tiled_windows(&session), vec!["a", "b", "d"]);
    assert_eq!(
        session.admission_sequence(&WindowId("b".to_owned())),
        seq_b,
        "refusal never re-sequences"
    );
}

#[test]
fn send_overlap_admits_and_preserves_follow_stay_and_order() {
    // Same geometry under allow-overlap: the send overlap-admits at the
    // ordinary anchor, marks the destination, keeps the mover leaf, and
    // follow/stay select destination-MRU vs source-MRU without changing
    // the admission order.
    for follow in [true, false] {
        let mut session = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![
                domain("o", "ws-1", 120, 80, 0),
                domain("o", "ws-2", 1, 1, 0),
            ],
        )
        .expect("session");
        session.set_no_space_result(NoSpaceResult::AllowOverlap);
        admit_and_commit(&mut session, "a", "o", "ws-1", placement(120, 40), "c-1");
        admit_and_commit(&mut session, "b", "o", "ws-1", placement(120, 40), "c-2");
        admit_and_commit(&mut session, "d", "o", "ws-2", placement(1, 1), "c-3");
        let seq_b = session.admission_sequence(&WindowId("b".to_owned()));
        let mover_leaf = leaf_of(&session, "b");
        let key1 = DomainKey {
            output: OutputId("o".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let key2 = DomainKey {
            output: OutputId("o".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
        };
        assert!(session.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
        let obs = complete_observation(&session, Vec::new());
        let plan = session
            .propose(
                &SessionCommand::MoveToWorkspace {
                    window: WindowId("b".to_owned()),
                    target_output: OutputId("o".to_owned()),
                    target_workspace: WorkspaceId("ws-2".to_owned()),
                    follow,
                },
                &obs,
                &correlation("s-1"),
                &LifecycleCapabilities::full(),
            )
            .expect("overlap send plans");
        assert!(
            plan.desired_geometry
                .iter()
                .filter(|g| g.output.0 == "o" && g.workspace.0 == "ws-2")
                .any(|g| g.window.0 == "b"),
            "mover has destination geometry"
        );
        commit_plan(&mut session, &plan, "s-1");
        assert!(session.overlap_accepted(&key2), "destination marked");
        assert_eq!(leaf_of(&session, "b"), mover_leaf, "leaf identity kept");
        assert_eq!(
            session.admission_sequence(&WindowId("b".to_owned())),
            seq_b,
            "moves never re-sequence"
        );
        if follow {
            assert_eq!(session.focus().0, Some(key2), "follow selects target");
        } else {
            assert_eq!(session.focus().0, Some(key1), "stay keeps source");
        }
    }
}

#[test]
fn r4_exhausted_floats_at_chosen_output_with_no_d2_redirect() {
    // Adjacent outputs: source out-1/ws sole mover m, target out-2/ws 1x1
    // with t. Under float the exact wrap plans first, proves geometric
    // exhaustion, and capacity-floats the mover at the CHOSEN output
    // (MoveToFloat under the explicit output-send intent, follow-only,
    // untouched frame, no D2 redirect); under overlap it keeps the exact
    // whole-target wrap with the mark. Both fallback settings agree: R4
    // never searches.
    use crate::contract::LifecycleOperation;
    use crate::directional::{Capabilities, Direction};
    for fallback in [
        crate::admission::AnchorFallback::LargestSplittable,
        crate::admission::AnchorFallback::NoSearch,
    ] {
        for (no_space, expect_ok) in [
            (NoSpaceResult::Float, false),
            (NoSpaceResult::AllowOverlap, true),
        ] {
            let mut d1 = domain("out-1", "ws", 120, 80, 0);
            let mut d2 = domain("out-2", "ws", 1, 1, 0);
            d1.adjacent
                .insert(Direction::Right, OutputId("out-2".to_owned()));
            d2.adjacent
                .insert(Direction::Left, OutputId("out-1".to_owned()));
            let mut session =
                Session::new(owner(), generation(), 0, 7, vec![d1.clone(), d2.clone()])
                    .expect("session");
            session.set_anchor_fallback(fallback);
            session.set_no_space_result(no_space);
            let m_plan =
                admit_and_commit(&mut session, "m", "out-1", "ws", placement(120, 40), "c-1");
            let _frame_m = m_plan
                .desired_geometry
                .iter()
                .find(|g| g.window.0 == "m")
                .expect("m projected at admission")
                .rect;
            admit_and_commit(&mut session, "t", "out-2", "ws", placement(1, 1), "c-2");
            let src = DomainKey {
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
            };
            let target_key = DomainKey {
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
            };
            let dest_tree_before = session.tree_for(&target_key).expect("target tree").clone();
            let seq_m = session.admission_sequence(&WindowId("m".to_owned()));
            assert!(session.sync_focus_from_window(&src, &WindowId("m".to_owned())));
            let mover_leaf = leaf_of(&session, "m");
            let t_leaf = leaf_of(&session, "t");
            let obs = complete_observation(&session, Vec::new());
            let outcome = session.propose_move(
                &src,
                &WindowId("m".to_owned()),
                Direction::Right,
                &obs,
                &correlation("m-1"),
                &Capabilities::full(),
            );
            if expect_ok {
                let plan = outcome
                    .expect("overlap R4 plans")
                    .expect_moved("tiled wrap");
                assert!(
                    plan.desired_geometry.iter().any(|g| g.window.0 == "m"),
                    "mover projects at the chosen output"
                );
                // Exact wrap: the target tree still contains the mover leaf
                // plus the prior occupant (no redirect elsewhere).
                let snapshot = &plan.desired_snapshot;
                let target_view = snapshot
                    .domains
                    .iter()
                    .find(|d| d.output.0 == "out-2")
                    .expect("target view");
                let tree = target_view.tree.clone().expect("target tree");
                fn leaves(node: &Node) -> Vec<String> {
                    match node {
                        Node::Leaf { id } => vec![id.0.clone()],
                        Node::Group { children, .. } => children.iter().flat_map(leaves).collect(),
                    }
                }
                assert!(
                    leaves(&tree).contains(&mover_leaf.0),
                    "wrap keeps the mover leaf in the target"
                );
                let _ = target_key;
            } else {
                let plan = outcome
                    .expect("float R4 stages the capacity fallback")
                    .capacity_float()
                    .expect("fallback is the slotless float, never a tiled plan");
                match &plan.dispatch.operation {
                    LifecycleOperation::MoveToFloat {
                        window,
                        leaf,
                        source_output,
                        source_workspace,
                        target_output,
                        target_workspace,
                    } => {
                        assert_eq!(window.0, "m");
                        assert_eq!(leaf, &mover_leaf, "dropped leaf named for audit");
                        assert_eq!(source_output.0, "out-1");
                        assert_eq!(source_workspace.0, "ws");
                        assert_eq!(target_output.0, "out-2", "chosen output kept");
                        assert_eq!(target_workspace.0, "ws");
                    }
                    other => panic!("float R4 carries MoveToFloat, got {other:?}"),
                }
                match &plan.dispatch.intent {
                    crate::contract::LifecycleIntent::MoveToOutput { follow, .. } => {
                        assert!(follow, "R4 always follows the mover");
                    }
                    other => panic!("float R4 rides the output-send intent, got {other:?}"),
                }
                assert!(
                    crate::boundary::SendWorkspacePlan::from_session(&plan).is_some(),
                    "existing send transfer-proof gate accepts the float shape"
                );
                assert!(
                    plan.desired_geometry.iter().all(|g| g.window.0 != "m"),
                    "slotless mover takes no tiled geometry: no native frame move"
                );
                assert!(
                    plan.desired_geometry.iter().any(|g| g.window.0 == "t"),
                    "survivor still projects"
                );
                // Follow names the target bookkeeping MRU (native follow
                // addresses the mover ref); a cross-output float stages no
                // source-coordinate frame.
                assert_eq!(
                    (
                        plan.desired_focus_domain.clone(),
                        plan.desired_focus_leaf.clone()
                    ),
                    (Some(target_key.clone()), Some(t_leaf.clone())),
                    "follow selects the target survivor bookkeeping"
                );
                assert!(session.has_pending() && session.has_pending_desired());
                commit_plan(&mut session, &plan, "m-1");
                assert_eq!(tiled_windows(&session), vec!["t".to_string()]);
                let record = session
                    .exception_observed()
                    .into_iter()
                    .find(|w| w.window.0 == "m")
                    .expect("mover floated");
                assert_eq!(record.output.0, "out-2", "homed at the chosen output");
                assert_eq!(record.workspace.0, "ws");
                assert!(
                    record.floating && !record.fullscreen && !record.maximized && !record.sticky
                );
                assert!(session.is_capacity_float(&WindowId("m".to_owned())));
                assert!(
                    !session.is_automatic_fixed_float(&WindowId("m".to_owned())),
                    "capacity origin stays distinct from fixed auto"
                );
                assert!(
                    session
                        .floating_geometry(&WindowId("m".to_owned()))
                        .is_none(),
                    "cross-output stages no source-coordinate claim: the frame \
                     stays untouched for the native remap plus arrival proof"
                );
                assert_eq!(
                    session.tree_for(&target_key),
                    Some(&dest_tree_before),
                    "destination tree untouched: no tile slot taken"
                );
                assert_eq!(
                    session.admission_sequence(&WindowId("m".to_owned())),
                    seq_m,
                    "moves never re-sequence"
                );
                assert_eq!(
                    session.focus(),
                    (Some(target_key.clone()), Some(t_leaf.clone())),
                    "committed follow stays on the target bookkeeping"
                );
                let _ = fallback;
            }
        }
    }
}

#[test]
fn r4_float_never_redirects_to_a_roomy_third_output() {
    // A third output with room does not divert the fallback: the float
    // homes at the R4-chosen output, never tiles elsewhere (no D2 even
    // when another leaf could fit).
    use crate::contract::LifecycleOperation;
    use crate::directional::{Capabilities, Direction};
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    let d3 = domain("out-3", "ws", 120, 80, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![d1.clone(), d2.clone(), d3.clone()],
    )
    .expect("session");
    admit_and_commit(&mut session, "m", "out-1", "ws", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "t", "out-2", "ws", placement(1, 1), "c-2");
    let src = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(session.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let obs = complete_observation(&session, Vec::new());
    let plan = session
        .propose_move(
            &src,
            &WindowId("m".to_owned()),
            Direction::Right,
            &obs,
            &correlation("m-1"),
            &Capabilities::full(),
        )
        .expect("float R4 stages the capacity fallback")
        .capacity_float()
        .expect("fallback is the slotless float");
    match &plan.dispatch.operation {
        LifecycleOperation::MoveToFloat {
            target_output,
            target_workspace,
            ..
        } => {
            assert_eq!(target_output.0, "out-2");
            assert_eq!(target_workspace.0, "ws");
        }
        other => panic!("float R4 carries MoveToFloat, got {other:?}"),
    }
    commit_plan(&mut session, &plan, "m-1");
    let record = session
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "m")
        .expect("mover floated");
    assert_eq!(
        record.output.0, "out-2",
        "never redirected to the roomy third output"
    );
    let third = DomainKey {
        output: OutputId("out-3".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert_eq!(session.tree_for(&third), None, "third output untouched");
    assert_eq!(tiled_windows(&session), vec!["t".to_string()]);
}

#[test]
fn r4_unrepresentable_source_refuses_without_mutation() {
    // The exact wrap plans, but the shrunken source no longer projects,
    // so the source remap fails: the fallback refuses truthfully with
    // the source exactly preserved instead of staging a float from
    // broken geometry (no fake success, no guessed frame).
    use crate::directional::{Capabilities, Direction};
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut session =
        Session::new(owner(), generation(), 0, 7, vec![d1.clone(), d2.clone()]).expect("session");
    admit_and_commit(&mut session, "a", "out-1", "ws", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "m", "out-1", "ws", placement(120, 40), "c-2");
    admit_and_commit(&mut session, "t", "out-2", "ws", placement(1, 1), "c-3");
    let src = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    // Shrink the source until its own retained tree stops projecting; no
    // repair runs, so the mover frame becomes unprovable.
    assert!(session.update_domain_gaps(
        &src,
        Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1
        },
        0
    ));
    assert!(session.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let mover_leaf = leaf_of(&session, "m");
    let (snapshot, focus, revision) = (
        session.snapshot(),
        session.focus(),
        session.accepted_revision(),
    );
    let obs = complete_observation(&session, Vec::new());
    assert!(
        matches!(
            session.propose_move(
                &src,
                &WindowId("m".to_owned()),
                Direction::Right,
                &obs,
                &correlation("m-1"),
                &Capabilities::full(),
            ),
            Err(crate::session::ProposeError::Refused(
                crate::session::RefusalKind::MalformedTopology
            ))
        ),
        "unprovable mover frame refuses instead of floating blindly"
    );
    assert!(!session.has_pending() && !session.has_pending_desired());
    assert_eq!(session.snapshot(), snapshot, "source preserved");
    assert_eq!(session.focus(), focus);
    assert_eq!(session.accepted_revision(), revision);
    assert_eq!(
        session.focus(),
        (Some(src), Some(mover_leaf)),
        "refusal preserves the mover focus"
    );
    assert!(!session.is_capacity_float(&WindowId("m".to_owned())));
}

#[test]
fn r4_float_withdrawn_before_ack_preserves_source() {
    // A staged R4 capacity-float withdrawn before acknowledgement commits
    // nothing: the mover stays tiled at the source, no exception or origin
    // leaks, focus and revision untouched.
    use crate::directional::{Capabilities, Direction};
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut session =
        Session::new(owner(), generation(), 0, 7, vec![d1.clone(), d2.clone()]).expect("session");
    admit_and_commit(&mut session, "m", "out-1", "ws", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "t", "out-2", "ws", placement(1, 1), "c-2");
    let src = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(session.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let (snapshot, focus, revision) = (
        session.snapshot(),
        session.focus(),
        session.accepted_revision(),
    );
    let obs = complete_observation(&session, Vec::new());
    let plan = session
        .propose_move(
            &src,
            &WindowId("m".to_owned()),
            Direction::Right,
            &obs,
            &correlation("m-1"),
            &Capabilities::full(),
        )
        .expect("float R4 stages the capacity fallback")
        .capacity_float()
        .expect("fallback is the slotless float");
    session
        .cancel_unacked_pending(&correlation("m-1"), plan.dispatch.base_revision)
        .expect("withdrawn before ack");
    assert_eq!(session.snapshot(), snapshot, "source preserved");
    assert_eq!(session.focus(), focus);
    assert_eq!(session.accepted_revision(), revision);
    assert_eq!(
        tiled_windows(&session),
        vec!["m".to_string(), "t".to_string()]
    );
    assert!(session.exception_observed().is_empty());
    assert!(!session.is_capacity_float(&WindowId("m".to_owned())));
}

#[test]
fn overlap_focus_and_resize_keep_working_with_truthful_hold() {
    // Overlap-accept tiny with a, b; switch back to float (mark retained).
    // Directional focus and keyboard resize still project through the
    // gate; an unknown-domain focus still refuses.
    use crate::contract::{FocusCapabilities, ResizeCapabilities};
    use crate::directional::Direction;
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("tiny", "ws", 1, 1, 0)],
    )
    .expect("session");
    session.set_no_space_result(NoSpaceResult::AllowOverlap);
    admit_and_commit(&mut session, "a", "tiny", "ws", placement(1, 1), "c-1");
    admit_and_commit(&mut session, "b", "tiny", "ws", placement(1, 1), "c-2");
    let key = DomainKey {
        output: OutputId("tiny".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(session.overlap_accepted(&key));
    session.set_no_space_result(NoSpaceResult::Float);
    assert!(session.overlap_accepted(&key), "retained after switch");
    let obs = complete_observation(&session, Vec::new());
    let focused = session
        .focus()
        .1
        .and_then(|leaf| {
            session
                .snapshot()
                .windows
                .iter()
                .find(|l| l.leaf == leaf)
                .map(|l| l.window.clone())
        })
        .expect("tiled focus");
    let other = if focused.0 == "a" {
        WindowId("b".to_owned())
    } else {
        WindowId("a".to_owned())
    };
    let _ = &other;
    let mut focus_plan = None;
    let mut focus_direction = Direction::Right;
    for direction in [
        Direction::Right,
        Direction::Left,
        Direction::Down,
        Direction::Up,
    ] {
        match session.propose_focus(
            &key,
            &focused,
            direction,
            &obs,
            &correlation("f-1"),
            &FocusCapabilities::full(),
        ) {
            Ok(plan) => {
                focus_plan = Some(plan);
                focus_direction = direction;
                break;
            }
            Err(crate::session::ProposeError::Refused(crate::session::RefusalKind::Unchanged))
            | Err(crate::session::ProposeError::Refused(
                crate::session::RefusalKind::PlannerNoop,
            )) => continue,
            Err(other) => panic!("focus works on accepted overlap: {other:?}"),
        }
    }
    let focus_plan = focus_plan.expect("some direction focuses on overlap");
    assert!(!focus_plan.desired_geometry.is_empty());
    let direction = focus_direction;
    // Focus stages pending; withdraw it before the resize leg so each
    // call site is validated independently.
    session
        .cancel_unacked_pending(&correlation("f-1"), focus_plan.dispatch.base_revision)
        .expect("focus pending withdraws");
    // Resize shares on the overlap domain projects (or refuses Unchanged
    // when the step is a noop), never MalformedTopology.
    let resize = session.propose_resize(
        &key,
        &focused,
        Direction::Right,
        crate::contract::ResizeMode::Outwards,
        0,
        &obs,
        &correlation("r-1"),
        &ResizeCapabilities::full(),
    );
    match resize {
        Ok(plan) => assert!(!plan.desired_geometry.is_empty()),
        Err(crate::session::ProposeError::Refused(kind)) => assert!(
            kind != crate::session::RefusalKind::MalformedTopology,
            "overlap resize never holds unrepresentable: {kind:?}"
        ),
        Err(other) => panic!("unexpected resize error: {other:?}"),
    }
    // Unknown domain still refuses.
    let unknown = DomainKey {
        output: OutputId("nope".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(matches!(
        session.propose_focus(
            &unknown,
            &other,
            direction,
            &obs,
            &correlation("f-2"),
            &FocusCapabilities::full(),
        ),
        Err(crate::session::ProposeError::Refused(
            crate::session::RefusalKind::UnknownDomain
        ))
    ));
}

#[test]
fn drag_preview_and_finish_agree_on_exhausted_cross_output() {
    // Source roomy with a, b; destination 1x1 with t. A cross-output drag
    // of b to the exhausted destination refuses the preview and
    // snap-backs the finish identically: same gate, no pending, source
    // preserved, no faked float.
    use crate::contract::DragCapabilities;
    use crate::directional::Direction;
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut session =
        Session::new(owner(), generation(), 0, 7, vec![d1.clone(), d2.clone()]).expect("session");
    admit_and_commit(&mut session, "a", "out-1", "ws", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "b", "out-1", "ws", placement(120, 40), "c-2");
    admit_and_commit(&mut session, "t", "out-2", "ws", placement(1, 1), "c-3");
    let src = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    let dest = DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(session.sync_focus_from_window(&src, &WindowId("b".to_owned())));
    let obs = complete_observation(&session, Vec::new());
    let _capture = session
        .begin_drag(&WindowId("b".to_owned()), &obs)
        .expect("drag begins on roomy source");
    let snapshot = session.snapshot();
    // Pointer deep in the destination work area (1x1 origin bounds).
    let preview = session.preview_drag_cross_output(&dest, 0, 0, &obs);
    assert!(
        matches!(preview, Err(crate::session::ProposeError::Refused(_))),
        "preview refuses honestly (resolve or projection stage), got {preview:?}"
    );
    // Center resolves as unsupported: the finish refuses (Err) while
    // edge/infeasible placements snap back (Ok). Either way there is no
    // plan, no pending, and no finish-only float: preview and finish
    // agree through the same resolver and gate.
    let release = session.drop_drag_cross_output(
        &dest,
        0,
        0,
        &obs,
        &correlation("d-1"),
        &DragCapabilities::full(),
    );
    match release {
        Ok(crate::session::DragRelease::SnapBack(_)) => {}
        Err(crate::session::ProposeError::Refused(_)) => {}
        other => panic!("finish refuses honestly, never a finish-only float: {other:?}"),
    }
    assert!(!session.has_pending() && !session.has_pending_desired());
    assert_eq!(session.snapshot(), snapshot, "source preserved");
}
