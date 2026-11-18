use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::CoreCommand;
use crate::contract::MAX_REVISION;
use crate::directional::{
    Axis, Direction, Node, NodeId, OutputId, Rule, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;
use crate::world_commands::CommandApplied;
use crate::world_engine::{EventIssue, KIND_OPERATION, KIND_SCOPE_HELD, StagedDomain, WorldEngine};

use super::outcome::{LayoutFailure, PairMoveOutcome, PairOperationFailure};
use super::pair_operation::PairOperationContext;
use super::state::{DomainException, DomainLayout, WorldLayout};

fn source_key() -> DomainKey {
    DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

fn target_key() -> DomainKey {
    DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

fn source_output_domain() -> OutputDomain {
    let mut adjacent = BTreeMap::new();
    adjacent.insert(Direction::Right, OutputId("out-2".to_owned()));
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 800,
        },
        gap: 0,
        adjacent,
    }
}

fn target_output_domain() -> OutputDomain {
    let mut adjacent = BTreeMap::new();
    adjacent.insert(Direction::Left, OutputId("out-1".to_owned()));
    OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 1000,
            y: 0,
            w: 1000,
            h: 800,
        },
        gap: 0,
        adjacent,
    }
}

fn leaf(id: &str) -> Node {
    Node::Leaf {
        id: NodeId(id.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn eng(window: &str, key: &DomainKey) -> EngineWindow {
    EngineWindow {
        window: win(window),
        output: key.output.clone(),
        workspace: key.workspace.clone(),
        rect: Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn domain_with(
    output_domain: &OutputDomain,
    tree: Node,
    leaves: &[(&str, &str)],
    focused: &str,
) -> DomainLayout {
    let mut domain = DomainLayout::new(output_domain.bounds, output_domain.gap, 0);
    domain.tree = Some(tree);
    for (w, l) in leaves {
        domain.leaves.insert(win(w), nid(l));
    }
    if !focused.is_empty() {
        domain.focused_leaf = Some(nid(focused));
        domain.focus_stack.push(nid(focused));
        domain.focus_history.push(nid(focused));
        domain.last_active = Some(nid(focused));
    }
    domain
}

#[test]
fn pair_focus_local_wins_before_cross_fallback() {
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source_key(),
        domain_with(
            &src_dom,
            Node::Group {
                id: nid("g1"),
                axis: Axis::Horizontal,
                children: vec![leaf("l1"), leaf("l2")],
                shares: vec![1, 1],
            },
            &[("w1", "l1"), ("w2", "l2")],
            "l1",
        ),
    );
    layout.domains.insert(
        target_key(),
        domain_with(&dst_dom, leaf("l3"), &[("w3", "l3")], "l3"),
    );

    let obs = vec![
        eng("w1", &source_key()),
        eng("w2", &source_key()),
        eng("w3", &target_key()),
    ];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_focus(&ctx, &focus, Direction::Right, false)
        .expect("local focus should win");
    assert!(!outcome.crossed);
    assert_eq!(outcome.from_window, win("w1"));
    assert_eq!(outcome.to_window, win("w2"));
    assert_eq!(outcome.target_output, source_key().output);
}

#[test]
fn pair_focus_unchanged_cross_fallback_selects_target_entry_edge() {
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source_key(),
        domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1"),
    );
    layout.domains.insert(
        target_key(),
        domain_with(
            &dst_dom,
            Node::Group {
                id: nid("g2"),
                axis: Axis::Horizontal,
                children: vec![leaf("l2"), leaf("l3")],
                shares: vec![1, 1],
            },
            &[("w2", "l2"), ("w3", "l3")],
            "l3",
        ),
    );

    let obs = vec![
        eng("w1", &source_key()),
        eng("w2", &target_key()),
        eng("w3", &target_key()),
    ];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_focus(&ctx, &focus, Direction::Right, false)
        .expect("cross focus should select entry-edge target");
    assert!(outcome.crossed);
    assert_eq!(outcome.from_window, win("w1"));
    assert_eq!(outcome.to_window, win("w2"));
    assert_eq!(outcome.target_output, target_key().output);
}

#[test]
fn pair_focus_maximized_cross_allows_vertical_and_fullscreen_refuses() {
    let mut src_dom = source_output_domain();
    src_dom
        .adjacent
        .insert(Direction::Down, OutputId("out-2".to_owned()));
    let mut dst_dom = target_output_domain();
    dst_dom
        .adjacent
        .insert(Direction::Up, OutputId("out-1".to_owned()));

    let mut layout = WorldLayout::new();
    layout.config.fixed_size_admission = true;
    layout.domains.insert(
        source_key(),
        domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1"),
    );
    layout.domains.insert(
        target_key(),
        domain_with(&dst_dom, leaf("l2"), &[("w2", "l2")], "l2"),
    );

    let mut w1_obs = eng("w1", &source_key());
    w1_obs.maximized = true;
    let obs = vec![w1_obs.clone(), eng("w2", &target_key())];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_focus(&ctx, &focus, Direction::Down, false)
        .expect("maximized non-fullscreen allows vertical cross");
    assert!(outcome.crossed);
    assert_eq!(outcome.to_window, win("w2"));

    // Fullscreen refuses vertical cross with Unchanged, while convergence persists.
    let mut w1_fs = eng("w1", &source_key());
    w1_fs.fullscreen = true;
    let obs_fs = vec![w1_fs, eng("w2", &target_key()), eng("w3", &target_key())];
    let ctx_fs = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs_fs,
        closed: &[],
    };
    let err = layout
        .run_directional_pair_focus(&ctx_fs, &focus, Direction::Down, false)
        .expect_err("fullscreen should not cross vertically");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    // Converged w3 remains installed in target domain despite command refusal.
    assert!(
        layout
            .domain(&target_key())
            .expect("target retained")
            .leaves
            .contains_key(&win("w3"))
    );
}

#[test]
fn pair_focus_float_subject_uses_remembered_target_and_binds_active_home() {
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();
    let mut layout = WorldLayout::new();
    let mut src_layout = DomainLayout::new(src_dom.bounds, src_dom.gap, 0);
    src_layout.exceptions.insert(
        win("wf"),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            floating_geometry: Some(Rect {
                x: 20,
                y: 20,
                w: 200,
                h: 200,
            }),
        },
    );
    layout.domains.insert(source_key(), src_layout);
    let mut dst_layout = domain_with(
        &dst_dom,
        Node::Group {
            id: nid("g2"),
            axis: Axis::Horizontal,
            children: vec![leaf("l2"), leaf("l3")],
            shares: vec![1, 1],
        },
        &[("w2", "l2"), ("w3", "l3")],
        "l3",
    );
    dst_layout.last_active = Some(nid("l3"));
    layout.domains.insert(target_key(), dst_layout);

    let mut wf_obs = eng("wf", &source_key());
    wf_obs.floating = true;
    let obs = vec![wf_obs, eng("w2", &target_key()), eng("w3", &target_key())];
    let focus = win("wf");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_focus(&ctx, &focus, Direction::Right, true)
        .expect("float subject should cross to remembered target");
    assert!(outcome.crossed);
    assert_eq!(outcome.from_leaf, None);
    assert_eq!(outcome.to_window, win("w3"));

    // Mismatched observed_focus refuses FocusMismatch.
    let wrong_focus = win("w2");
    let bad_ctx = PairOperationContext {
        observed_focus: &wrong_focus,
        ..ctx
    };
    let err = layout
        .run_directional_pair_focus(&bad_ctx, &focus, Direction::Right, true)
        .expect_err("subject != observed_focus must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
}

#[test]
fn pair_move_r2a_local_and_same_axis_swap() {
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source_key(),
        domain_with(
            &src_dom,
            Node::Group {
                id: nid("g1"),
                axis: Axis::Horizontal,
                children: vec![leaf("l1"), leaf("l2"), leaf("l3")],
                shares: vec![1, 1, 1],
            },
            &[("w1", "l1"), ("w2", "l2"), ("w3", "l3")],
            "l1",
        ),
    );
    layout.domains.insert(
        target_key(),
        domain_with(&dst_dom, leaf("l4"), &[("w4", "l4")], "l4"),
    );

    let obs = vec![
        eng("w1", &source_key()),
        eng("w2", &source_key()),
        eng("w3", &source_key()),
        eng("w4", &target_key()),
    ];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_move(
            &ctx,
            &focus,
            Direction::Right,
            true,
            SameAxisMove::SwapWithNeighbor,
        )
        .expect("R2c swap should succeed");
    match outcome {
        PairMoveOutcome::Moved {
            rule,
            cross,
            window,
            ..
        } => {
            assert_eq!(rule, Rule::R2c);
            assert!(cross.is_none());
            assert_eq!(window, win("w1"));
        }
        PairMoveOutcome::CapacityFloat { .. } => panic!("expected tiled move"),
    }
}

#[test]
fn pair_move_r4_cross_and_capacity_float_fallback() {
    let src_dom = source_output_domain();
    let mut dst_dom = target_output_domain();
    dst_dom.bounds = Rect {
        x: 1000,
        y: 0,
        w: 1000,
        h: 800,
    };

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source_key(),
        domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1"),
    );
    layout.domains.insert(
        target_key(),
        domain_with(&dst_dom, leaf("l2"), &[("w2", "l2")], "l2"),
    );

    let obs = vec![eng("w1", &source_key()), eng("w2", &target_key())];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(8),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    // Without cross_output_transfer capability, R4 refuses.
    let err = layout
        .run_directional_pair_move(
            &ctx,
            &focus,
            Direction::Right,
            false,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect_err("disabled cross_output_transfer must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::UnsupportedCapability))
    );

    // With cross_output_transfer enabled, R4 succeeds and sets target outer gap.
    let outcome = layout
        .run_directional_pair_move(
            &ctx,
            &focus,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("R4 move should succeed");
    match outcome {
        PairMoveOutcome::Moved { rule, cross, .. } => {
            assert_eq!(rule, Rule::R4);
            let cross = cross.expect("R4 emits cross metadata");
            assert_eq!(cross.target_output, target_key().output);
        }
        PairMoveOutcome::CapacityFloat { .. } => panic!("expected tiled R4 move"),
    }
    assert!(layout.domain(&source_key()).is_none());
    let target = layout.domain(&target_key()).expect("target retained");
    assert_eq!(target.outer_gap, 8);
    assert!(target.leaves.contains_key(&win("w1")));

    // Capacity-float fallback: width-1 target fits one leaf before move but cannot
    // project two positive horizontal child widths after R4 wrap.
    let mut narrow_dst = target_output_domain();
    narrow_dst.bounds = Rect {
        x: 1000,
        y: 0,
        w: 1,
        h: 800,
    };
    let mut layout2 = WorldLayout::new();
    layout2.domains.insert(
        source_key(),
        domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1"),
    );
    layout2.domains.insert(
        target_key(),
        domain_with(&narrow_dst, leaf("l2"), &[("w2", "l2")], "l2"),
    );
    let obs_narrow = vec![eng("w1", &source_key()), eng("w2", &target_key())];
    let ctx_narrow = PairOperationContext {
        target_domain: &narrow_dst,
        target_outer_gap: Some(0),
        observed: &obs_narrow,
        ..ctx
    };
    let outcome2 = layout2
        .run_directional_pair_move(
            &ctx_narrow,
            &focus,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("R4 geometric exhaustion should fall back to capacity float");
    match outcome2 {
        PairMoveOutcome::CapacityFloat {
            window,
            target_output,
            follow,
            ..
        } => {
            assert_eq!(window, win("w1"));
            assert_eq!(target_output, target_key().output);
            assert!(follow);
        }
        PairMoveOutcome::Moved { .. } => panic!("expected capacity-float fallback"),
    }
    let target2 = layout2.domain(&target_key()).expect("target retained");
    assert!(target2.capacity_floats.contains(&win("w1")));
    assert!(target2.exceptions.contains_key(&win("w1")));
}

#[test]
fn pair_revision_limit_refuses_atomically_while_convergence_persists() {
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();
    let mut layout = WorldLayout::new();
    let mut src_layout = domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1");
    src_layout.accepted_revision = MAX_REVISION - 1;
    layout.domains.insert(source_key(), src_layout);
    layout.domains.insert(
        target_key(),
        domain_with(&dst_dom, leaf("l2"), &[("w2", "l2")], "l2"),
    );

    let obs = vec![
        eng("w1", &source_key()),
        eng("w2", &target_key()),
        eng("w3", &target_key()),
    ];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let err = layout
        .run_directional_pair_move(
            &ctx,
            &focus,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect_err("MAX_REVISION must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        })
    );
    // Converged w3 persists even though the move command refused at MAX_REVISION.
    assert!(
        layout
            .domain(&target_key())
            .expect("target retained")
            .leaves
            .contains_key(&win("w3"))
    );
}

#[test]
fn pair_move_r4_and_capacity_float_preserve_fixed_override_and_admission_order() {
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();

    // 1) R4 tiled move preserves source fixed_tile_override, automatic_fixed, and admission_order.
    let mut layout = WorldLayout::new();
    let mut src_layout = domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1");
    src_layout.fixed_tile_override.insert(win("w1"));
    src_layout.automatic_fixed.insert(win("w1"));
    src_layout.admission_order.insert(win("w1"), 7);
    layout.domains.insert(source_key(), src_layout);
    let mut dst_layout = domain_with(&dst_dom, leaf("l2"), &[("w2", "l2")], "l2");
    dst_layout.admission_order.insert(win("w2"), 12);
    layout.domains.insert(target_key(), dst_layout);

    let mut w1_obs = eng("w1", &source_key());
    w1_obs.fixed_auto = true;
    w1_obs.fixed_suppress = true;
    let obs = vec![w1_obs.clone(), eng("w2", &target_key())];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_move(
            &ctx,
            &focus,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("R4 move should succeed");
    assert!(matches!(
        outcome,
        PairMoveOutcome::Moved { rule: Rule::R4, .. }
    ));
    let target = layout.domain(&target_key()).expect("target retained");
    assert!(target.fixed_tile_override.contains(&win("w1")));
    assert!(target.automatic_fixed.contains(&win("w1")));
    assert_eq!(target.admission_order.get(&win("w1")), Some(&7));
    assert_eq!(target.admission_order.get(&win("w2")), Some(&12));

    // 2) R4 capacity-float fallback (width-1 target exhaustion) also preserves
    // same-lifetime order and provenance.
    let mut narrow_dst = target_output_domain();
    narrow_dst.bounds = Rect {
        x: 1000,
        y: 0,
        w: 1,
        h: 800,
    };
    let mut layout2 = WorldLayout::new();
    let mut src2 = domain_with(&src_dom, leaf("l1"), &[("w1", "l1")], "l1");
    src2.fixed_tile_override.insert(win("w1"));
    src2.automatic_fixed.insert(win("w1"));
    src2.admission_order.insert(win("w1"), 3);
    layout2.domains.insert(source_key(), src2);
    let mut dst2 = domain_with(&narrow_dst, leaf("l2"), &[("w2", "l2")], "l2");
    dst2.admission_order.insert(win("w2"), 9);
    layout2.domains.insert(target_key(), dst2);

    let obs_narrow = vec![w1_obs, eng("w2", &target_key())];
    let ctx_narrow = PairOperationContext {
        target_domain: &narrow_dst,
        target_outer_gap: Some(0),
        observed: &obs_narrow,
        ..ctx
    };
    let outcome2 = layout2
        .run_directional_pair_move(
            &ctx_narrow,
            &focus,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("R4 capacity float should succeed");
    assert!(matches!(outcome2, PairMoveOutcome::CapacityFloat { .. }));
    let target2 = layout2.domain(&target_key()).expect("target retained");
    assert!(target2.capacity_floats.contains(&win("w1")));
    assert!(target2.fixed_tile_override.contains(&win("w1")));
    assert!(target2.automatic_fixed.contains(&win("w1")));
    assert_eq!(target2.admission_order.get(&win("w1")), Some(&3));
    assert_eq!(target2.admission_order.get(&win("w2")), Some(&9));
}

#[test]
fn world_directional_pair_refusal_priority_and_preflight_order() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let issue = EventIssue {
        owner: OwnerId::parse("owner-1").expect("valid owner"),
        correlation: CorrelationId::parse("corr-1").expect("valid correlation"),
    };
    let src_key = source_key();
    let dst_key = target_key();

    // 1) Missing source row refuses KIND_OPERATION before direction parsing.
    let empty_by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
    let empty_held: BTreeSet<DomainKey> = BTreeSet::new();
    let missing_focus = CoreCommand::Focus {
        window: "missing".to_owned(),
        direction: Direction::Right,
        cross_output_transfer: true,
        float_subject: false,
    };
    assert!(matches!(
        engine.tx(|tx| tx.execute_operation(&issue, 1, &missing_focus, &empty_by_id, &empty_held)),
        CommandApplied::Rejected {
            kind: KIND_OPERATION,
            ..
        }
    ));

    // 2) Held source scope refuses KIND_SCOPE_HELD before direction parsing and opaque-id gate.
    let bad_row = eng("bad/id", &src_key);
    let mut by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
    by_id.insert(&bad_row.window, &bad_row);
    let mut held_src = BTreeSet::new();
    held_src.insert(src_key.clone());
    let held_move = CoreCommand::Move {
        window: "bad/id".to_owned(),
        direction: Direction::Right,
        cross_output_transfer: true,
        same_axis_move: SameAxisMove::GroupWithNeighbor,
    };
    assert!(matches!(
        engine.tx(|tx| tx.execute_operation(&issue, 2, &held_move, &by_id, &held_src)),
        CommandApplied::Rejected {
            kind: KIND_SCOPE_HELD,
            ..
        }
    ));

    // 3) Pair world preflight runs before is_opaque_id gate:
    //    without staged pair/topology, non-opaque id still refuses KIND_OPERATION.
    let bad_id_focus = CoreCommand::Focus {
        window: "bad/id".to_owned(),
        direction: Direction::Right,
        cross_output_transfer: true,
        float_subject: false,
    };
    assert!(matches!(
        engine.tx(|tx| tx.execute_operation(&issue, 3, &bad_id_focus, &by_id, &empty_held)),
        CommandApplied::Rejected {
            kind: KIND_OPERATION,
            ..
        }
    ));

    // Once world preflight succeeds, is_opaque_id gate fires for Focus and Move.
    let src_dom = source_output_domain();
    let dst_dom = target_output_domain();
    engine
        .world
        .workspace
        .live_topology
        .insert(src_key.output.0.clone(), src_dom.bounds);
    engine
        .world
        .workspace
        .live_topology
        .insert(dst_key.output.0.clone(), dst_dom.bounds);
    engine.world.staged.domains = vec![
        StagedDomain {
            key: src_key.clone(),
            bounds: src_dom.bounds,
            gap: 0,
            outer_gap: 0,
            windows: vec![bad_row.clone()],
            focused: bad_row.window.clone(),
            minimized: Vec::new(),
            unreadable: Vec::new(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        },
        StagedDomain {
            key: dst_key.clone(),
            bounds: dst_dom.bounds,
            gap: 0,
            outer_gap: 0,
            windows: vec![eng("w2", &dst_key)],
            focused: win("w2"),
            minimized: Vec::new(),
            unreadable: Vec::new(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        },
    ];
    assert!(matches!(
        engine.tx(|tx| tx.execute_operation(&issue, 4, &bad_id_focus, &by_id, &empty_held)),
        CommandApplied::Rejected {
            kind: "focus-window-invalid",
            ..
        }
    ));
    let bad_id_move = CoreCommand::Move {
        window: "bad/id".to_owned(),
        direction: Direction::Right,
        cross_output_transfer: true,
        same_axis_move: SameAxisMove::GroupWithNeighbor,
    };
    assert!(matches!(
        engine.tx(|tx| tx.execute_operation(&issue, 5, &bad_id_move, &by_id, &empty_held)),
        CommandApplied::Rejected {
            kind: "move-window-invalid",
            ..
        }
    ));
}

#[test]
fn focus_route_sixty_five_levels_deep_succeeds_without_count_cap() {
    let k = source_key();
    let obs_for = |window: &str| crate::session::ObservedWindow {
        window: win(window),
        output: k.output.clone(),
        workspace: k.workspace.clone(),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    };
    let mut layout = WorldLayout::new();
    let mut leaves = BTreeMap::new();
    let mut focus_stack = Vec::new();
    let mut observed = Vec::new();

    // Build a 65-level deep nested tree on the right branch of root:
    // g0(Horizontal: [leaf-0, g1]), g1(Horizontal: [g2, leaf-1]), ...,
    // g64(Horizontal: [leaf-64, leaf-65]).
    // Focusing Right from leaf-0 descends g1 -> g2 -> ... -> g64 -> leaf-64,
    // yielding a route with >= 65 nodes.
    let l64 = NodeId("leaf-64".to_owned());
    let l65 = NodeId("leaf-65".to_owned());
    let w64 = win("w64");
    let w65 = win("w65");
    leaves.insert(w64.clone(), l64.clone());
    leaves.insert(w65.clone(), l65.clone());
    focus_stack.push(l64.clone());
    focus_stack.push(l65.clone());
    observed.push(obs_for("w64"));
    observed.push(obs_for("w65"));

    let mut current = Node::Group {
        id: NodeId("g64".to_owned()),
        axis: Axis::Horizontal,
        children: vec![Node::Leaf { id: l64 }, Node::Leaf { id: l65 }],
        shares: vec![1, 1],
    };

    for i in (1..64).rev() {
        let lid = NodeId(format!("leaf-{i}"));
        let wid = win(&format!("w{i}"));
        leaves.insert(wid.clone(), lid.clone());
        focus_stack.push(lid.clone());
        observed.push(obs_for(&format!("w{i}")));
        current = Node::Group {
            id: NodeId(format!("g{i}")),
            axis: Axis::Horizontal,
            children: vec![current, Node::Leaf { id: lid }],
            shares: vec![500, 1],
        };
    }

    let l0 = NodeId("leaf-0".to_owned());
    let w0 = win("w0");
    leaves.insert(w0.clone(), l0.clone());
    focus_stack.push(l0.clone());
    observed.push(obs_for("w0"));

    let root = Node::Group {
        id: NodeId("g0".to_owned()),
        axis: Axis::Horizontal,
        children: vec![Node::Leaf { id: l0.clone() }, current],
        shares: vec![1, 1],
    };

    layout.domains.insert(
        k.clone(),
        DomainLayout {
            tree: Some(root),
            leaves,
            bounds: Rect {
                x: 0,
                y: 0,
                w: 20_000,
                h: 800,
            },
            gap: 0,
            outer_gap: 0,
            exceptions: BTreeMap::new(),
            focused_leaf: Some(l0),
            focus_stack,
            focus_history: Vec::new(),
            last_active: None,
            retained_float_geometry: BTreeMap::new(),
            automatic_fixed: BTreeSet::new(),
            fixed_tile_override: BTreeSet::new(),
            capacity_floats: BTreeSet::new(),
            admission_order: BTreeMap::new(),
            minimized_slots: BTreeMap::new(),
            accepted_revision: 1,
        },
    );

    let outcome = layout
        .local_focus(&k, &w0, Direction::Right, &w0, &observed)
        .expect("65-level focus route succeeds without count cap");
    assert_eq!(outcome.base_revision, 1);
    assert_eq!(outcome.revision, 2);
    assert_eq!(outcome.to_window, w64);
    assert!(
        outcome.route.len() >= 65,
        "expected route len >= 65, got {}",
        outcome.route.len()
    );
}
