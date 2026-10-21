//! Send-float results and admission order: exhausted-float mover frame, failed-ack preservation, deferred carry, and sequence stability.
//!
//! Shared helpers live in the parent (`capacity_admission.rs`); this module
//! holds only `#[test]` fns.

use super::*;

#[test]
fn send_exhausted_floats_mover_with_exact_frame() {
    // Default float result: a send to an exhausted destination succeeds
    // as a slotless capacity float instead of refusing. The mover keeps
    // its exact current frame (no native move), the source allocation is
    // released, the destination tree is untouched, the origin is
    // diagnostic capacity, the admission order never re-sequences, and
    // the plan passes the existing send transfer-proof gate.
    // The mover carries size hints so the hinted candidate evaluation
    // resolves through the mover leaf mapping.
    use crate::size_hints::WindowSizeHints;
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
        admit_and_commit(&mut session, "a", "o", "ws-1", placement(120, 40), "c-1");
        let b_plan = {
            let mut extra = tiled_observed("b", "o", "ws-1");
            extra.hints = WindowSizeHints {
                min_w: Some(200),
                min_h: None,
                max_w: None,
                max_h: None,
            };
            let obs = complete_observation(&session, vec![extra]);
            let plan = session
                .propose(
                    &admit_command("b", "o", "ws-1", placement(120, 40)),
                    &obs,
                    &correlation("c-2"),
                    &LifecycleCapabilities::full(),
                )
                .expect("b tiles despite minima");
            commit_plan(&mut session, &plan, "c-2");
            plan
        };
        let frame_b = b_plan
            .desired_geometry
            .iter()
            .find(|g| g.window.0 == "b")
            .expect("b projected at admission")
            .rect;
        admit_and_commit(&mut session, "d", "o", "ws-2", placement(1, 1), "c-3");
        let dest_tree_before = session
            .tree_for(&DomainKey {
                output: OutputId("o".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
            })
            .expect("destination tree")
            .clone();
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
            .expect("exhausted send floats under default float");
        match &plan.dispatch.operation {
            crate::contract::LifecycleOperation::MoveToFloat {
                window,
                leaf,
                source_output,
                target_workspace,
                ..
            } => {
                assert_eq!(window.0, "b");
                assert_eq!(leaf, &mover_leaf, "dropped leaf named for audit");
                assert_eq!(source_output.0, "o");
                assert_eq!(target_workspace.0, "ws-2");
            }
            other => panic!("float send carries MoveToFloat, got {other:?}"),
        }
        assert!(
            crate::boundary::SendWorkspacePlan::from_session(&plan).is_some(),
            "existing send transfer-proof gate accepts the float shape"
        );
        assert!(
            plan.desired_geometry.iter().all(|g| g.window.0 != "b"),
            "slotless mover takes no tiled geometry: no native frame move"
        );
        commit_plan(&mut session, &plan, "s-1");
        assert_eq!(tiled_windows(&session), vec!["a", "d"]);
        let record = session
            .exception_observed()
            .into_iter()
            .find(|w| w.window.0 == "b")
            .expect("mover floated");
        assert_eq!(record.output.0, "o");
        assert_eq!(record.workspace.0, "ws-2");
        assert!(record.floating && !record.fullscreen && !record.maximized && !record.sticky);
        assert!(session.is_capacity_float(&WindowId("b".to_owned())));
        assert!(
            !session.is_automatic_fixed_float(&WindowId("b".to_owned())),
            "capacity origin stays distinct from fixed auto"
        );
        assert_eq!(
            session.floating_geometry(&WindowId("b".to_owned())),
            Some(frame_b),
            "exact current frame preserved verbatim"
        );
        assert_eq!(
            session.retained_float_geometry(&WindowId("b".to_owned())),
            Some(frame_b)
        );
        assert_eq!(
            session.tree_for(&key2),
            Some(&dest_tree_before),
            "destination tree untouched: no tile slot taken"
        );
        assert_eq!(
            session.admission_sequence(&WindowId("b".to_owned())),
            seq_b,
            "moves never re-sequence"
        );
        if follow {
            assert_eq!(
                session.focus(),
                (Some(key2), Some(leaf_of(&session, "d"))),
                "follow names the target bookkeeping MRU"
            );
        } else {
            assert_eq!(
                session.focus(),
                (Some(key1), Some(leaf_of(&session, "a"))),
                "stay falls back through the source MRU"
            );
        }
    }
}

#[test]
fn send_float_failed_ack_preserves_source() {
    // A staged capacity-float send withdrawn before acknowledgement
    // commits nothing: the mover stays tiled at the source, no exception
    // or origin leaks, focus and revision untouched.
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
    let plan = session
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o".to_owned()),
                target_workspace: WorkspaceId("ws-2".to_owned()),
                follow: true,
            },
            &obs,
            &correlation("s-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("float send stages");
    session
        .cancel_unacked_pending(&correlation("s-1"), plan.dispatch.base_revision)
        .expect("withdrawn before ack");
    assert_eq!(session.snapshot(), snapshot, "source preserved");
    assert_eq!(session.focus(), focus);
    assert_eq!(session.accepted_revision(), revision);
    assert_eq!(tiled_windows(&session), vec!["a", "b", "d"]);
    assert!(session.exception_observed().is_empty());
    assert!(!session.is_capacity_float(&WindowId("b".to_owned())));
}

#[test]
fn capacity_float_send_then_deferred_carries_ordinary_status() {
    // A capacity-floated arrival sends again through the ordinary
    // deferred path (same eligibility as an intentional float): the
    // slotless carry rehomes it with the diagnostic origin riding the
    // lifetime, and tiled survivors never reflow.
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
    let key1 = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    assert!(session.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let obs = complete_observation(&session, Vec::new());
    let plan = session
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o".to_owned()),
                target_workspace: WorkspaceId("ws-2".to_owned()),
                follow: true,
            },
            &obs,
            &correlation("s-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("float send stages");
    commit_plan(&mut session, &plan, "s-1");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    // The floated arrival carries onward like any ordinary float.
    let obs = complete_observation(&session, Vec::new());
    let carry = session
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o".to_owned()),
                target_workspace: WorkspaceId("ws-1".to_owned()),
                follow: false,
            },
            &obs,
            &correlation("s-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("capacity float carries through the ordinary deferred path");
    match &carry.dispatch.operation {
        crate::contract::LifecycleOperation::MoveDeferred { window, .. } => {
            assert_eq!(window.0, "b");
        }
        other => panic!("float carry stays slotless deferred, got {other:?}"),
    }
    commit_plan(&mut session, &carry, "s-2");
    let record = session
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "b")
        .expect("still floated");
    assert_eq!(record.workspace.0, "ws-1", "rehomed with the lifetime");
    assert!(
        session.is_capacity_float(&WindowId("b".to_owned())),
        "diagnostic origin rides the lifetime across moves"
    );
    assert_eq!(tiled_windows(&session), vec!["a", "d"]);
}

#[test]
fn converge_batch_records_carried_admission_order_not_id_order() {
    // HIGH-1: a batched [z-win, a-win] observation must record z=0, a=1
    // (carried admission order), never id order (a=0, z=1). A later
    // shrink then evicts the actual newest (a-win), not the id-first
    // window.
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("o", "w", 120, 80, 0)],
    )
    .expect("session");
    let obs = SessionObservation {
        observation: Observation::new(owner(), generation(), 0, 42),
        windows: vec![
            tiled_observed("z-win", "o", "w"),
            tiled_observed("a-win", "o", "w"),
        ],
    };
    let counts = session
        .converge_observation(&obs, Some(&WindowId("a-win".to_owned())))
        .expect("batch converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 2, 0)
    );
    assert_eq!(
        session.admission_sequence(&WindowId("z-win".to_owned())),
        Some(0),
        "first in carried order takes the first sequence"
    );
    assert_eq!(
        session.admission_sequence(&WindowId("a-win".to_owned())),
        Some(1),
        "second in carried order takes the second sequence, never id order"
    );
    // Shrink until exactly one victim must float: the actual newest.
    let key = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("w".to_owned()),
    };
    assert!(session.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 80
        },
        0
    ));
    assert!(session.repair_infeasible_domain(
        &key,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        true,
    ));
    assert_eq!(tiled_windows(&session), vec!["z-win".to_string()]);
    assert!(
        session.is_capacity_float(&WindowId("a-win".to_owned())),
        "shrink evicts the actual newest first"
    );
}

#[test]
fn clean_fit_roster_shares_one_sequence_with_identity_tiebreak() {
    // A clean-fit roster shares one admission sequence number, so later
    // victim selection breaks the tie by window identity descending.
    use crate::directional::{Axis, WindowLink};
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("o", "w", 200, 100, 0)],
    )
    .expect("session");
    let tree = Node::Group {
        id: NodeId("fit-g0".to_owned()),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId("fit-l0".to_owned()),
            },
            Node::Leaf {
                id: NodeId("fit-l1".to_owned()),
            },
        ],
        shares: vec![100, 100],
    };
    let links = vec![
        WindowLink {
            window: WindowId("a".to_owned()),
            leaf: NodeId("fit-l0".to_owned()),
            output: OutputId("o".to_owned()),
            workspace: WorkspaceId("w".to_owned()),
        },
        WindowLink {
            window: WindowId("b".to_owned()),
            leaf: NodeId("fit-l1".to_owned()),
            output: OutputId("o".to_owned()),
            workspace: WorkspaceId("w".to_owned()),
        },
    ];
    let obs = complete_observation(
        &session,
        vec![tiled_observed("a", "o", "w"), tiled_observed("b", "o", "w")],
    );
    let plan = session
        .propose_fitted_admit(
            tree,
            links,
            NodeId("fit-l0".to_owned()),
            &WindowId("a".to_owned()),
            &OutputId("o".to_owned()),
            &WorkspaceId("w".to_owned()),
            &obs,
            &correlation("f-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("clean fit stages");
    commit_plan(&mut session, &plan, "f-1");
    assert_eq!(
        session.admission_sequence(&WindowId("a".to_owned())),
        session.admission_sequence(&WindowId("b".to_owned())),
        "clean-fit roster shares one sequence number"
    );
    assert_eq!(
        session.admission_sequence(&WindowId("a".to_owned())),
        Some(0)
    );
    // Shrink until one victim must float: the shared sequence ties, so
    // identity descending evicts b first.
    let key = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("w".to_owned()),
    };
    assert!(session.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 100
        },
        0
    ));
    assert!(session.repair_infeasible_domain(
        &key,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        true,
    ));
    assert_eq!(tiled_windows(&session), vec!["a".to_string()]);
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
}

#[test]
fn workspace_send_preserves_global_admission_order() {
    // Transfer keeps the mover's first sequence globally: a later fresh
    // admission in either domain takes the next counter, never a
    // collision with the moved lifetime.
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("o", "ws-1", 120, 80, 0),
            domain("o", "ws-2", 120, 80, 0),
        ],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "o", "ws-1", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "b", "o", "ws-1", placement(120, 40), "c-2");
    admit_and_commit(&mut session, "d", "o", "ws-2", placement(120, 40), "c-3");
    assert_eq!(
        session.admission_sequence(&WindowId("b".to_owned())),
        Some(1)
    );
    let key1 = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    assert!(session.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let obs = complete_observation(&session, Vec::new());
    let plan = session
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o".to_owned()),
                target_workspace: WorkspaceId("ws-2".to_owned()),
                follow: true,
            },
            &obs,
            &correlation("s-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("send plans");
    commit_plan(&mut session, &plan, "s-1");
    assert_eq!(
        session.admission_sequence(&WindowId("b".to_owned())),
        Some(1),
        "transfer keeps the first sequence"
    );
    admit_and_commit(&mut session, "c", "o", "ws-1", placement(120, 40), "c-4");
    assert_eq!(
        session.admission_sequence(&WindowId("c".to_owned())),
        Some(3),
        "fresh admission advances the global counter past the moved lifetime"
    );
    assert_eq!(
        session.admission_sequence(&WindowId("d".to_owned())),
        Some(2)
    );
}

#[test]
fn send_to_float_same_output_keeps_native_live_frame() {
    // MED-2: the engine-carried native live frame is the exact current
    // native frame. It wins over the projected allocation (which can
    // differ under B6/clamped hints) instead of assuming equality.
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
    let b_plan = admit_and_commit(&mut session, "b", "o", "ws-1", placement(120, 40), "c-2");
    let allocation = b_plan
        .desired_geometry
        .iter()
        .find(|g| g.window.0 == "b")
        .expect("b projected at admission")
        .rect;
    admit_and_commit(&mut session, "d", "o", "ws-2", placement(1, 1), "c-3");
    // A live frame the allocation never equals (discriminating input).
    let live = Rect {
        x: 1,
        y: 2,
        w: 30,
        h: 30,
    };
    assert_ne!(live, allocation, "live frame differs from the allocation");
    let key1 = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    assert!(session.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let obs = complete_observation(&session, Vec::new());
    let plan = session
        .propose_send_with_live_frame(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o".to_owned()),
                target_workspace: WorkspaceId("ws-2".to_owned()),
                follow: true,
            },
            &obs,
            &correlation("s-1"),
            &LifecycleCapabilities::full(),
            Some(live),
        )
        .expect("exhausted send floats");
    assert!(
        matches!(
            &plan.dispatch.operation,
            crate::contract::LifecycleOperation::MoveToFloat { .. }
        ),
        "exhausted destination capacity-floats"
    );
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "b"),
        "slotless mover takes no tiled geometry"
    );
    commit_plan(&mut session, &plan, "s-1");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    assert_eq!(
        session.floating_geometry(&WindowId("b".to_owned())),
        Some(live),
        "same-output float keeps the exact native live frame"
    );
    assert_eq!(
        session.retained_float_geometry(&WindowId("b".to_owned())),
        Some(live)
    );
}

#[test]
fn send_to_float_cross_output_stages_no_source_coordinate_claim() {
    // MED-2: side-by-side outputs with nonzero origins. A cross-output
    // capacity float never translates, centers, or otherwise guesses the
    // source coordinates into a target claim: the frame stays untouched
    // (None) for the established native remap plus its arrival proof,
    // even when a live frame was carried.
    let out1 = OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let out2 = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 1000,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let mut session = Session::new(owner(), generation(), 0, 7, vec![out1, out2]).expect("session");
    admit_and_commit(&mut session, "m", "out-1", "ws", placement(120, 40), "c-1");
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
    assert!(session.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let obs = complete_observation(&session, Vec::new());
    // A source-coordinate live frame must not leak into the target,
    // with or without a carried frame.
    for (corr, live) in [
        (
            "x-1",
            Some(Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            }),
        ),
        ("x-2", None),
    ] {
        let mut probe = session.clone();
        let plan = probe
            .propose_send_with_live_frame(
                &SessionCommand::MoveToOutput {
                    window: WindowId("m".to_owned()),
                    target_output: OutputId("out-2".to_owned()),
                    target_workspace: WorkspaceId("ws".to_owned()),
                    follow: true,
                },
                &obs,
                &correlation(corr),
                &LifecycleCapabilities::full(),
                live,
            )
            .expect("exhausted cross-output send floats");
        assert!(
            matches!(
                &plan.dispatch.operation,
                crate::contract::LifecycleOperation::MoveToFloat { .. }
            ),
            "exhausted destination capacity-floats"
        );
        commit_plan(&mut probe, &plan, corr);
        assert_eq!(tiled_windows(&probe), vec!["t".to_string()]);
        assert!(probe.is_capacity_float(&WindowId("m".to_owned())));
        assert_eq!(
            probe.floating_geometry(&WindowId("m".to_owned())),
            None,
            "no source-coordinate claim at the target (live={live:?})"
        );
        assert_eq!(
            probe.retained_float_geometry(&WindowId("m".to_owned())),
            None
        );
        let record = probe
            .exception_observed()
            .into_iter()
            .find(|w| w.window.0 == "m")
            .expect("mover floated");
        assert_eq!(record.output.0, "out-2", "homed at the target");
        assert_eq!(
            probe.tree_for(&target_key),
            Some(&dest_tree_before),
            "destination tree untouched"
        );
    }
}

#[test]
fn send_to_output_refused_when_target_unknown_source_preserved() {
    // Unavailable remap target: the output send refuses UnknownDomain
    // with the source exactly preserved and no re-sequencing.
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![
            domain("o1", "ws", 120, 80, 0),
            domain("o2", "ws", 120, 80, 0),
        ],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "o1", "ws", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "b", "o1", "ws", placement(120, 40), "c-2");
    let seq_b = session.admission_sequence(&WindowId("b".to_owned()));
    let key1 = DomainKey {
        output: OutputId("o1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(session.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let (snapshot, focus, revision) = (
        session.snapshot(),
        session.focus(),
        session.accepted_revision(),
    );
    let obs = complete_observation(&session, Vec::new());
    let err = session
        .propose_send_with_live_frame(
            &SessionCommand::MoveToOutput {
                window: WindowId("b".to_owned()),
                target_output: OutputId("o9".to_owned()),
                target_workspace: WorkspaceId("ws".to_owned()),
                follow: true,
            },
            &obs,
            &correlation("x-1"),
            &LifecycleCapabilities::full(),
            Some(Rect {
                x: 0,
                y: 0,
                w: 60,
                h: 80,
            }),
        )
        .expect_err("unknown target refuses");
    assert_eq!(
        err,
        crate::session::ProposeError::Refused(crate::session::RefusalKind::UnknownDomain)
    );
    assert_eq!(session.snapshot(), snapshot, "source preserved");
    assert_eq!(session.focus(), focus);
    assert_eq!(session.accepted_revision(), revision);
    assert!(!session.has_pending() && !session.has_pending_desired());
    assert_eq!(
        session.admission_sequence(&WindowId("b".to_owned())),
        seq_b,
        "refusal never re-sequences"
    );
    assert!(
        session.exception_observed().is_empty(),
        "no fake float staged"
    );
}
