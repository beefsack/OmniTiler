use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::CoreCommand;
use crate::directional::{Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{EventIssue, KIND_OPERATION, StagedDomain, WorldEngine};

use super::outcome::{LayoutFailure, PairOperationFailure};
use super::pair::PairAssemblyFailure;
use super::pair_operation::PairOperationContext;
use super::state::{DomainException, DomainLayout, WorldLayout};

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_owned())
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

fn eng(window: &str, k: &DomainKey, floating: bool) -> EngineWindow {
    EngineWindow {
        window: win(window),
        output: k.output.clone(),
        workspace: k.workspace.clone(),
        rect: Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn tiled_domain(dom: &OutputDomain, window: &str, leaf_id: &str) -> DomainLayout {
    let mut d = DomainLayout::new(dom.bounds, dom.gap, 0);
    d.tree = Some(Node::Leaf { id: nid(leaf_id) });
    d.leaves.insert(win(window), nid(leaf_id));
    d.focused_leaf = Some(nid(leaf_id));
    d.focus_stack.push(nid(leaf_id));
    d.focus_history.push(nid(leaf_id));
    d.last_active = Some(nid(leaf_id));
    d
}

fn add_float_exception(d: &mut DomainLayout, window: &str) {
    d.exceptions.insert(
        win(window),
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
                w: 120,
                h: 80,
            }),
        },
    );
}

#[test]
fn float_cross_plans_remembered_target_and_commits_without_mutation() {
    let d1 = domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")]);
    let d2 = domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")]);
    let k1 = d1.key();
    let k2 = d2.key();

    let mut layout = WorldLayout::new();
    let mut src = tiled_domain(&d1, "win-a", "la");
    add_float_exception(&mut src, "win-f");
    let dst = tiled_domain(&d2, "win-x", "lx");
    layout.domains.insert(k1.clone(), src);
    layout.domains.insert(k2.clone(), dst);

    let before_src_tree = layout.domain(&k1).unwrap().tree.clone();
    let before_src_leaves = layout.domain(&k1).unwrap().leaves.clone();
    let before_dst_tree = layout.domain(&k2).unwrap().tree.clone();
    let before_dst_leaves = layout.domain(&k2).unwrap().leaves.clone();

    let obs = vec![
        eng("win-a", &k1, false),
        eng("win-f", &k1, true),
        eng("win-x", &k2, false),
    ];
    let focus = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &d1,
        target_domain: &d2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_focus(&ctx, &win("win-f"), Direction::Right, true)
        .expect("float cross plans");
    assert!(outcome.crossed);
    assert_eq!(outcome.target_output, OutputId("out-2".to_owned()));
    assert_eq!(outcome.target_workspace, WorkspaceId("ws-b".to_owned()));
    assert_eq!(outcome.from_window, win("win-f"));
    // Honestly leafless: no tile leaf is borrowed for the float subject.
    assert_eq!(outcome.from_leaf, None);
    assert_eq!(outcome.to_window, win("win-x"));
    assert_eq!(outcome.route, vec![outcome.to_leaf.clone()]);
    // No float geometry is fabricated: every desired entry is a tile.
    for entry in &outcome.geometry {
        assert_ne!(entry.window, win("win-f"));
    }

    assert_eq!(layout.focus_leaf(&k2), Some(&nid("lx")));
    assert_eq!(layout.domain(&k1).unwrap().tree, before_src_tree);
    assert_eq!(layout.domain(&k1).unwrap().leaves, before_src_leaves);
    assert_eq!(layout.domain(&k2).unwrap().tree, before_dst_tree);
    assert_eq!(layout.domain(&k2).unwrap().leaves, before_dst_leaves);
}

#[test]
fn float_cross_needs_no_source_tile() {
    let d1 = domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")]);
    let d2 = domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")]);
    let k1 = d1.key();
    let k2 = d2.key();

    let mut layout = WorldLayout::new();
    let mut src = DomainLayout::new(d1.bounds, d1.gap, 0);
    add_float_exception(&mut src, "win-f");
    let dst = tiled_domain(&d2, "win-x", "lx");
    layout.domains.insert(k1.clone(), src);
    layout.domains.insert(k2.clone(), dst);
    assert!(layout.domain(&k1).unwrap().leaves.is_empty());

    let obs = vec![eng("win-f", &k1, true), eng("win-x", &k2, false)];
    let focus = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &d1,
        target_domain: &d2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_directional_pair_focus(&ctx, &win("win-f"), Direction::Right, true)
        .expect("float-only cross plans");
    assert!(outcome.crossed);
    assert_eq!(outcome.target_output, OutputId("out-2".to_owned()));
    assert_eq!(outcome.target_workspace, WorkspaceId("ws-b".to_owned()));
    assert_eq!(outcome.from_window, win("win-f"));
    assert_eq!(outcome.from_leaf, None);
    assert_eq!(outcome.to_window, win("win-x"));
    assert_eq!(layout.focus_leaf(&k2), Some(&nid("lx")));
}

#[test]
fn float_cross_subject_eligibility_refuses_fail_closed() {
    let d1 = domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")]);
    let d2 = domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")]);
    let k1 = d1.key();
    let k2 = d2.key();

    let mut layout = WorldLayout::new();
    let mut src = tiled_domain(&d1, "win-a", "la");
    add_float_exception(&mut src, "win-f");
    let dst = tiled_domain(&d2, "win-x", "lx");
    layout.domains.insert(k1.clone(), src);
    layout.domains.insert(k2.clone(), dst);

    let obs = vec![
        eng("win-a", &k1, false),
        eng("win-f", &k1, true),
        eng("win-x", &k2, false),
    ];
    let focus_f = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &d1,
        target_domain: &d2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus_f,
        observed: &obs,
        closed: &[],
    };

    // Unflagged cross keeps refusing exception subjects: tile-origin path unchanged.
    assert_eq!(
        layout.run_directional_pair_focus(&ctx, &win("win-f"), Direction::Right, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::NotTiled
        )))
    );
    assert_eq!(layout.focus_leaf(&k1), Some(&nid("la")));

    // A tiled subject carrying the float flag refuses as a mismatch.
    assert_eq!(
        layout.run_directional_pair_focus(&ctx, &win("win-a"), Direction::Right, true),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        )))
    );
    assert_eq!(layout.focus_leaf(&k1), Some(&nid("la")));

    // Unknown subjects refuse before crossing.
    let ghost_focus = win("win-ghost");
    let ghost_ctx = PairOperationContext {
        observed_focus: &ghost_focus,
        ..ctx
    };
    assert_eq!(
        layout.run_directional_pair_focus(&ghost_ctx, &win("win-ghost"), Direction::Right, true),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        )))
    );
    assert_eq!(layout.focus_leaf(&k1), Some(&nid("la")));

    // Observed disagreeing with known exception flags converges win-f to tiled
    // before the command, so float_subject=true refuses as FocusMismatch.
    let unflagged_obs = vec![
        eng("win-a", &k1, false),
        eng("win-f", &k1, false),
        eng("win-x", &k2, false),
    ];
    let bad_ctx = PairOperationContext {
        observed: &unflagged_obs,
        ..ctx
    };
    assert_eq!(
        layout.run_directional_pair_focus(&bad_ctx, &win("win-f"), Direction::Right, true),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        )))
    );
    assert_eq!(layout.focus_leaf(&k1), Some(&nid("leaf-win-f")));
    assert!(
        layout
            .domain(&k1)
            .unwrap()
            .leaves
            .contains_key(&win("win-f"))
    );
}

#[test]
fn float_cross_edge_conditions_refuse_without_mutation() {
    let d1 = domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")]);
    let d2 = domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")]);
    let k1 = d1.key();
    let k2 = d2.key();

    let mut layout = WorldLayout::new();
    let mut src = tiled_domain(&d1, "win-a", "la");
    add_float_exception(&mut src, "win-f");
    let dst = tiled_domain(&d2, "win-x", "lx");
    layout.domains.insert(k1.clone(), src);
    layout.domains.insert(k2.clone(), dst);

    let obs = vec![
        eng("win-a", &k1, false),
        eng("win-f", &k1, true),
        eng("win-x", &k2, false),
    ];
    let focus_f = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &d1,
        target_domain: &d2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus_f,
        observed: &obs,
        closed: &[],
    };

    // Up/Down never cross from floats either.
    for direction in [Direction::Up, Direction::Down] {
        assert_eq!(
            layout.run_directional_pair_focus(&ctx, &win("win-f"), direction, true),
            Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
                RefusalKind::Unchanged
            )))
        );
        assert_eq!(layout.focus_leaf(&k1), Some(&nid("la")));
    }

    // Missing adjacency refuses at both layout pair assembly and WorldEngine boundary.
    let lone_dom = domain_ws("out-1", "ws-a", vec![]);
    let lone_ctx = PairOperationContext {
        source_domain: &lone_dom,
        target_domain: &d2,
        ..ctx
    };
    assert_eq!(
        layout.run_directional_pair_focus(&lone_ctx, &win("win-f"), Direction::Right, true),
        Err(PairOperationFailure::Assembly(
            PairAssemblyFailure::DomainMismatch
        ))
    );

    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let issue = EventIssue {
        owner: OwnerId::parse("owner-1").expect("valid owner"),
        correlation: CorrelationId::parse("corr-1").expect("valid correlation"),
    };
    engine
        .world
        .workspace
        .live_topology
        .insert("out-1".to_owned(), lone_dom.bounds);
    let win_a = eng("win-a", &k1, false);
    let win_f = eng("win-f", &k1, true);
    engine.world.staged.domains = vec![StagedDomain {
        key: k1.clone(),
        bounds: lone_dom.bounds,
        gap: lone_dom.gap,
        outer_gap: 0,
        windows: vec![win_a.clone(), win_f.clone()],
        focused: win("win-f"),
        minimized: Vec::new(),
        unreadable: Vec::new(),
        closed: Vec::new(),
        readable: true,
        hold: None,
    }];
    let mut by_id = BTreeMap::new();
    by_id.insert(&win_a.window, &win_a);
    by_id.insert(&win_f.window, &win_f);
    let held = BTreeSet::new();
    let cmd = CoreCommand::Focus {
        window: "win-f".to_owned(),
        direction: "right".to_owned(),
        cross_output_transfer: true,
        float_subject: true,
    };
    assert!(matches!(
        engine.tx(|tx| tx.execute_operation(&issue, 1, &cmd, &by_id, &held)),
        crate::world_commands::CommandApplied::Rejected {
            kind: KIND_OPERATION,
            ..
        }
    ));

    // An empty target output refuses without crossing.
    let mut empty_layout = WorldLayout::new();
    let mut src2 = tiled_domain(&d1, "win-a", "la");
    add_float_exception(&mut src2, "win-f");
    empty_layout.domains.insert(k1.clone(), src2);
    empty_layout
        .domains
        .insert(k2.clone(), DomainLayout::new(d2.bounds, d2.gap, 0));

    let empty_obs = vec![eng("win-a", &k1, false), eng("win-f", &k1, true)];
    let empty_ctx = PairOperationContext {
        source_domain: &d1,
        target_domain: &d2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus_f,
        observed: &empty_obs,
        closed: &[],
    };
    assert_eq!(
        empty_layout.run_directional_pair_focus(&empty_ctx, &win("win-f"), Direction::Right, true),
        Err(PairOperationFailure::Assembly(
            PairAssemblyFailure::TargetUnusable
        ))
    );
    assert_eq!(empty_layout.focus_leaf(&k1), Some(&nid("la")));

    // A target output with no tiled leaves (only floating) refuses Unchanged.
    let mut float_only_dst = DomainLayout::new(d2.bounds, d2.gap, 0);
    add_float_exception(&mut float_only_dst, "win-y");
    empty_layout.domains.insert(k2.clone(), float_only_dst);
    let float_dst_obs = vec![
        eng("win-a", &k1, false),
        eng("win-f", &k1, true),
        eng("win-y", &k2, true),
    ];
    let float_dst_ctx = PairOperationContext {
        observed: &float_dst_obs,
        ..empty_ctx
    };
    assert_eq!(
        empty_layout.run_directional_pair_focus(
            &float_dst_ctx,
            &win("win-f"),
            Direction::Right,
            true
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::Unchanged
        )))
    );
    assert_eq!(empty_layout.focus_leaf(&k1), Some(&nid("la")));
}
