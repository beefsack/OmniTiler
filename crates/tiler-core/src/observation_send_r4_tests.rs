#![forbid(unsafe_code)]
//! Step-3 send/R4 immediate-commit rows (test-only, authorized).
//!
//! Design: docs/changes/observation-convergence-step-3.md. `WorldLayout`
//! commits a planned send/R4 topology synchronously into the canonical
//! per-domain layouts (retaining survivor trees) and returns both-domain
//! geometry. Failed native moves converge on the next complete observations
//! with no phantom.

use std::collections::BTreeMap;

use crate::directional::{
    Direction, Node, NodeId, OutputId, Rule, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind};
use crate::world_layout::{
    LayoutFailure, PairMoveOutcome, PairOperationContext, PairOperationFailure, WorldLayout,
};

fn bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }
}
fn domain(output: &str, workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: bounds(),
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn pair_domain(output: &str, workspace: &str, neighbor: (Direction, &str)) -> OutputDomain {
    let mut d = domain(output, workspace);
    d.adjacent
        .insert(neighbor.0, OutputId(neighbor.1.to_owned()));
    d
}
fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}
fn carried(window: &str, output: &str, workspace: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x,
            y: 0,
            w: 10,
            h: 10,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn observed_tiled(window: &str, output: &str, workspace: &str) -> ObservedWindow {
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
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn admit_to(layout: &mut WorldLayout, d: &OutputDomain, window: &str) {
    let dk = d.key();
    if !layout.contains_domain(&dk) {
        layout
            .reconcile_fresh(
                d,
                0,
                &WindowId(window.to_owned()),
                &[carried(window, &d.id.0, &d.workspace.0, 0)],
            )
            .unwrap_or_else(|e| panic!("seed fresh {window}: {e:?}"));
        return;
    }
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(&dk)
        .map(|w| observed_tiled(&w.0, &d.id.0, &d.workspace.0))
        .collect();
    windows.push(observed_tiled(window, &d.id.0, &d.workspace.0));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    layout
        .admit_window(
            &dk,
            &WindowId(window.to_owned()),
            ExceptionFlags::none(),
            None,
            d.bounds,
            false,
            false,
            &windows,
        )
        .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
}
fn leaf_of(layout: &WorldLayout, o: &str, ws: &str, w: &str) -> NodeId {
    layout
        .leaves(&key(o, ws))
        .and_then(|leaves| leaves.get(&WindowId(w.to_owned())))
        .expect("leaf for window")
        .clone()
}
fn leaves_of(layout: &WorldLayout, o: &str, ws: &str) -> Vec<String> {
    fn go(n: &Node, by_leaf: &BTreeMap<NodeId, String>, out: &mut Vec<String>) {
        match n {
            Node::Leaf { id } => out.push(by_leaf.get(id).cloned().unwrap_or_else(|| id.0.clone())),
            Node::Group { children, .. } => children.iter().for_each(|c| go(c, by_leaf, out)),
        }
    }
    let dk = key(o, ws);
    let by_leaf: BTreeMap<NodeId, String> = layout
        .leaves(&dk)
        .map(|leaves| {
            leaves
                .iter()
                .map(|(w, l)| (l.clone(), w.0.clone()))
                .collect()
        })
        .unwrap_or_default();
    match layout.tree(&dk) {
        None => vec![],
        Some(t) => {
            let mut v = vec![];
            go(t, &by_leaf, &mut v);
            v
        }
    }
}
fn homed_in(layout: &WorldLayout, o: &str, ws: &str, w: &str) -> bool {
    let dk = key(o, ws);
    let wid = WindowId(w.to_owned());
    layout.is_tiled_window(&dk, &wid) || layout.is_exception(&dk, &wid)
}
fn reconcile_domain(
    layout: &mut WorldLayout,
    d: &OutputDomain,
    outer_gap: i32,
    windows: Vec<EngineWindow>,
    focused: &str,
) {
    let dk = d.key();
    let focused_id = WindowId(focused.to_owned());
    layout
        .converge_retained_observation(&dk, &windows, &focused_id, &[])
        .unwrap_or_else(|e| panic!("converge {dk:?}: {e:?}"));
    match layout
        .reconcile_retained(&dk, d.bounds, d.gap, outer_gap, &focused_id, &windows, &[])
        .unwrap_or_else(|e| panic!("retained reconcile {dk:?}: {e:?}"))
    {
        Some(_) => {}
        None => {
            layout
                .reconcile_fresh(d, outer_gap, &focused_id, &windows)
                .unwrap_or_else(|e| panic!("fresh reconcile {dk:?}: {e:?}"));
        }
    }
}
fn seed_send_layout() -> (WorldLayout, OutputDomain, OutputDomain) {
    let mut layout = WorldLayout::new();
    let source_domain = domain("out-1", "ws-a");
    admit_to(&mut layout, &source_domain, "win-s1");
    admit_to(&mut layout, &source_domain, "win-s2");
    admit_to(&mut layout, &source_domain, "win-m");
    let target_domain = domain("out-1", "ws-b");
    admit_to(&mut layout, &target_domain, "win-t1");
    admit_to(&mut layout, &target_domain, "win-t2");
    (layout, source_domain, target_domain)
}
fn send_pair_observed() -> Vec<EngineWindow> {
    // Scrambled rects: spatial reseed would order survivors as
    // [s2, s1] / [t2, t1]; retained topology must stay [s1, s2] / [t1, t2].
    vec![
        carried("win-s2", "out-1", "ws-a", 0),
        carried("win-s1", "out-1", "ws-a", 200),
        carried("win-m", "out-1", "ws-a", 400),
        carried("win-t2", "out-1", "ws-b", 0),
        carried("win-t1", "out-1", "ws-b", 400),
    ]
}

#[test]
fn send_commits_immediately_with_retained_topology_and_stays() {
    let (mut layout, source_domain, target_domain) = seed_send_layout();
    let src_base = layout.accepted_revision(&key("out-1", "ws-a"));
    let tgt_base = layout.accepted_revision(&key("out-1", "ws-b"));
    let obs = send_pair_observed();
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    let geometry = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("send must commit")
        .geometry;
    // Retained survivor order preserved (not spatial-reseeded), mover moved.
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["win-s1", "win-s2"]
    );
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-b"),
        vec!["win-t1", "win-t2", "win-m"]
    );
    assert!(!homed_in(&layout, "out-1", "ws-a", "win-m"));
    assert!(homed_in(&layout, "out-1", "ws-b", "win-m"));
    assert!(layout.accepted_revision(&key("out-1", "ws-a")) > src_base);
    assert!(layout.accepted_revision(&key("out-1", "ws-b")) > tgt_base);
    // Both-domain geometry plus the native assignment.
    let mut ids: Vec<String> = geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["win-m", "win-s1", "win-s2", "win-t1", "win-t2"]);
    assert!(
        geometry
            .iter()
            .any(|g| g.window.0 == "win-m" && g.output.0 == "out-1" && g.workspace.0 == "ws-b")
    );
    // Native arrival stays: complete observations preserve both topologies.
    let src_obs = vec![
        carried("win-s1", "out-1", "ws-a", 0),
        carried("win-s2", "out-1", "ws-a", 10),
    ];
    let tgt_obs = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
        carried("win-m", "out-1", "ws-b", 20),
    ];
    reconcile_domain(&mut layout, &source_domain, 0, src_obs, "win-s2");
    reconcile_domain(&mut layout, &target_domain, 0, tgt_obs, "win-m");
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["win-s1", "win-s2"]
    );
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-b"),
        vec!["win-t1", "win-t2", "win-m"]
    );
}

#[test]
fn send_stay_keeps_source_selected_with_mru_focus() {
    let (mut layout, source_domain, target_domain) = seed_send_layout();
    let obs = send_pair_observed();
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, false, false)
        .expect("stay must commit");
    assert!(!outcome.follow);
    // Source stays selected: the MRU survivor keeps focus, never the target.
    assert_eq!(
        outcome.stay_focus_window.as_ref().map(|w| w.0.as_str()),
        Some("win-s2")
    );
    assert_eq!(
        outcome.focus_leaf.as_ref().map(|leaf| leaf.0.as_str()),
        Some(leaf_of(&layout, "out-1", "ws-a", "win-s2").0.as_str())
    );
    // Destination admission is unchanged from the follow path.
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["win-s1", "win-s2"]
    );
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-b"),
        vec!["win-t1", "win-t2", "win-m"]
    );
    assert!(homed_in(&layout, "out-1", "ws-b", "win-m"));
    assert_eq!(
        layout
            .focus_leaf(&key("out-1", "ws-a"))
            .as_ref()
            .map(|l| l.0.as_str()),
        Some(leaf_of(&layout, "out-1", "ws-a", "win-s2").0.as_str())
    );
    // Both-domain geometry with the native assignment on the target.
    let mut ids: Vec<String> = outcome
        .geometry
        .iter()
        .map(|g| g.window.0.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["win-m", "win-s1", "win-s2", "win-t1", "win-t2"]);
    assert!(
        outcome
            .geometry
            .iter()
            .any(|g| g.window.0 == "win-m" && g.output.0 == "out-1" && g.workspace.0 == "ws-b")
    );
}

#[test]
fn failed_native_send_converges_to_source_without_phantom() {
    let (mut layout, source_domain, target_domain) = seed_send_layout();
    let obs = send_pair_observed();
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("send must commit");
    assert!(
        homed_in(&layout, "out-1", "ws-b", "win-m"),
        "native failure must follow an actual provisional commit"
    );
    // Native refused: mover still observed on source, target without mover.
    let src_obs = vec![
        carried("win-s1", "out-1", "ws-a", 0),
        carried("win-s2", "out-1", "ws-a", 10),
        carried("win-m", "out-1", "ws-a", 20),
    ];
    let tgt_obs = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
    ];
    reconcile_domain(&mut layout, &source_domain, 0, src_obs, "win-m");
    reconcile_domain(&mut layout, &target_domain, 0, tgt_obs, "win-t2");
    // Provisional target leaf withdrawn; mover admitted normally on source.
    assert!(homed_in(&layout, "out-1", "ws-a", "win-m"));
    assert!(!homed_in(&layout, "out-1", "ws-b", "win-m"));
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["win-s1", "win-s2", "win-m"]
    );
    assert!(
        !leaves_of(&layout, "out-1", "ws-b").contains(&"win-m".to_owned()),
        "no retained phantom on target"
    );
    for window in ["win-s1", "win-s2", "win-m", "win-t1", "win-t2"] {
        assert_eq!(
            usize::from(homed_in(&layout, "out-1", "ws-a", window))
                + usize::from(homed_in(&layout, "out-1", "ws-b", window)),
            1
        );
    }
}

#[test]
fn r4_cross_output_commits_immediately_with_empty_target() {
    // Source out-1/ws-a holds H[win-a, win-m] with win-m focused at the right
    // edge; target out-2/ws-b is empty (no retained domain).
    let mut layout = WorldLayout::new();
    let source_base = domain("out-1", "ws-a");
    admit_to(&mut layout, &source_base, "win-a");
    admit_to(&mut layout, &source_base, "win-m");
    let source_pair = pair_domain("out-1", "ws-a", (Direction::Right, "out-2"));
    let target_pair = pair_domain("out-2", "ws-b", (Direction::Left, "out-1"));
    let obs = vec![
        carried("win-a", "out-1", "ws-a", 0),
        carried("win-m", "out-1", "ws-a", 10),
    ];
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_pair,
        target_domain: &target_pair,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    let geometry = match layout.run_directional_pair_move(
        &ctx,
        &focus,
        Direction::Right,
        true,
        SameAxisMove::GroupWithNeighbor,
    ) {
        Ok(PairMoveOutcome::Moved { rule, geometry, .. }) => {
            assert_eq!(rule, Rule::R4);
            geometry
        }
        other => panic!("R4 must plan cross-output, got {other:?}"),
    };
    assert_eq!(leaves_of(&layout, "out-1", "ws-a"), vec!["win-a"]);
    assert_eq!(leaves_of(&layout, "out-2", "ws-b"), vec!["win-m"]);
    assert!(homed_in(&layout, "out-2", "ws-b", "win-m"));
    let mut ids: Vec<String> = geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["win-a", "win-m"]);
    assert!(geometry.iter().all(|g| g.rect.w > 0 && g.rect.h > 0));
}

#[test]
fn r4_exhausted_target_floats_mover_as_send_output() {
    // Source out-1/ws-a holds H[win-a, win-m] with win-m focused at the
    // right edge; target out-2/ws-b is an exhausted 1x1 holding win-t. The
    // exact R4 wrap proves geometric exhaustion under the default float
    // result, so WorldLayout commits the shared capacity-float fallback
    // synchronously (never a refusal, never a redirect): the mover floats
    // slotless at the chosen target, the source releases, and the
    // destination tree is untouched.
    let mut layout = WorldLayout::new();
    let source_base = domain("out-1", "ws-a");
    admit_to(&mut layout, &source_base, "win-a");
    admit_to(&mut layout, &source_base, "win-m");
    let tiny_target = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-b".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: [(Direction::Left, OutputId("out-1".to_owned()))]
            .into_iter()
            .collect(),
    };
    let tiny_component = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-b".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    admit_to(&mut layout, &tiny_component, "win-t");
    let source_pair = pair_domain("out-1", "ws-a", (Direction::Right, "out-2"));
    let target_pair = tiny_target;
    let target_key = target_pair.key();
    let obs = vec![
        carried("win-a", "out-1", "ws-a", 0),
        carried("win-m", "out-1", "ws-a", 10),
        carried("win-t", "out-2", "ws-b", 0),
    ];
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_pair,
        target_domain: &target_pair,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    let geometry = match layout.run_directional_pair_move(
        &ctx,
        &focus,
        Direction::Right,
        true,
        SameAxisMove::GroupWithNeighbor,
    ) {
        Ok(PairMoveOutcome::CapacityFloat {
            window,
            target_output,
            target_workspace,
            follow,
            geometry,
            ..
        }) => {
            assert_eq!(window.0, "win-m");
            assert_eq!(target_output.0, "out-2");
            assert_eq!(target_workspace.0, "ws-b");
            assert!(follow, "R4 always follows the mover");
            geometry
        }
        other => panic!("exhausted R4 returns capacity-float fallback, got {other:?}"),
    };
    let mut ids: Vec<String> = geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["win-a", "win-t"],
        "survivors project, mover slotless"
    );
    assert_eq!(leaves_of(&layout, "out-1", "ws-a"), vec!["win-a"]);
    assert_eq!(leaves_of(&layout, "out-2", "ws-b"), vec!["win-t"]);
    assert!(layout.is_capacity_float(&target_key, &WindowId("win-m".to_owned())));
    assert!(
        !homed_in(&layout, "out-1", "ws-a", "win-m"),
        "source released"
    );
}

#[test]
fn rapid_second_send_with_arrival_commits_without_reconcile() {
    // Step-3 Watch risk probe, arrival-showing case: first send A->B commits;
    // then, with NO intervening per-domain reconcile but native observations
    // already showing the arrival (m on B), a second send B->C (empty C)
    // commits immediately with the native assignment. No refusal, no
    // pending-exists/abandon wait.
    let (mut layout, source_domain, target_domain) = seed_send_layout();
    let obs = send_pair_observed();
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("first send must commit");
    assert!(homed_in(&layout, "out-1", "ws-b", "win-m"));
    let source_b = target_domain;
    let target_c = domain("out-1", "ws-c");
    let b_windows = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
        carried("win-m", "out-1", "ws-b", 20),
    ];
    let second_ctx = PairOperationContext {
        source_domain: &source_b,
        target_domain: &target_c,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &b_windows,
        closed: &[],
    };
    match layout.run_pair_send(&second_ctx, &focus, true, false) {
        Ok(outcome) => {
            assert!(
                outcome.geometry.iter().any(|g| g.window.0 == "win-m"
                    && g.output.0 == "out-1"
                    && g.workspace.0 == "ws-c"),
                "second send must carry the native assignment, got {:?}",
                outcome.geometry
            );
            assert!(homed_in(&layout, "out-1", "ws-c", "win-m"));
        }
        other => panic!("PROBE-RESULT rapid second send refused: {other:?}"),
    }
}

#[test]
fn stale_rapid_second_send_drops_mover_until_domain_reconcile() {
    // Step-3 Watch risk, stale-observation case: after A->B commits, a rapid
    // B->C send of win-m itself carries a STALE B observation (m absent,
    // still natively on A) before any per-domain reconcile. Pair convergence
    // drops the provisional m from the canonical layouts (stored before the
    // focus-sync refusal), so the second send refuses with focus-mismatch
    // and m is retained NOWHERE until the domains ordinary-reconcile. The
    // refusal clears on complete observations and the command is NOT lost
    // silently indefinitely: reconciling A (m present) re-admits m and a
    // retried A->C send commits.
    let (mut layout, source_domain, target_domain) = seed_send_layout();
    let obs = send_pair_observed();
    let focus = WindowId("win-m".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("first send must commit");
    let source_b = target_domain.clone();
    let target_c = domain("out-1", "ws-c");
    let stale_b_windows = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
    ];
    let second_ctx = PairOperationContext {
        source_domain: &source_b,
        target_domain: &target_c,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &stale_b_windows,
        closed: &[],
    };
    match layout.run_pair_send(&second_ctx, &focus, true, false) {
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::FocusMismatch))) => {
        }
        other => panic!("stale rapid second send must refuse at focus sync, got {other:?}"),
    }
    // The converged removal was stored before the refusal: m is retained
    // nowhere, even though it is still natively on A.
    assert!(
        !homed_in(&layout, "out-1", "ws-b", "win-m"),
        "stale convergence drops provisional m from B"
    );
    assert!(
        !homed_in(&layout, "out-1", "ws-a", "win-m"),
        "m is retained nowhere until domain reconcile"
    );
    // Ordinary per-domain reconciles recover: A re-admits m normally.
    let a_obs = vec![
        carried("win-s1", "out-1", "ws-a", 0),
        carried("win-s2", "out-1", "ws-a", 10),
        carried("win-m", "out-1", "ws-a", 20),
    ];
    let b_obs = vec![
        carried("win-t1", "out-1", "ws-b", 0),
        carried("win-t2", "out-1", "ws-b", 10),
    ];
    reconcile_domain(&mut layout, &source_domain, 0, a_obs.clone(), "win-m");
    reconcile_domain(&mut layout, &target_domain, 0, b_obs, "win-t2");
    assert!(
        homed_in(&layout, "out-1", "ws-a", "win-m"),
        "complete A observation re-admits m"
    );
    // Retried send A->C (empty C) commits: the refused command was not lost
    // silently indefinitely.
    let retry_ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_c,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &a_obs,
        closed: &[],
    };
    match layout.run_pair_send(&retry_ctx, &focus, true, false) {
        Ok(outcome) => assert!(
            outcome
                .geometry
                .iter()
                .any(|g| g.window.0 == "win-m" && g.output.0 == "out-1" && g.workspace.0 == "ws-c"),
            "retried send must carry the native assignment, got {:?}",
            outcome.geometry
        ),
        other => panic!("retried send must commit after reconcile, got {other:?}"),
    }
    assert!(homed_in(&layout, "out-1", "ws-c", "win-m"));
}

// Causal state mirrors run-01dd52e26d954c9c act106/act109: the mover leaves
// ws-1 for ws-4, then returns with NO ws-1 reconcile in between, so
// last_active[ws-1] still names the departed mover leaf while the focus MRU
// holds the valid survivor. Ordinary open order (notepad, paint, terminal)
// via WorldLayout reconcile builds the same causal focus history as the
// live spatial seed plus focus events: stack [notepad, paint], stale
// last_active terminal. Live physical inputs: 2560x1380 work area, gap 8,
// outer 8, minima notepad 401x246, paint 864x617, terminal 582x95. Carried
// rects are dummy full-bounds (topology derives from retained layout;
// hints drive projection), unlike live native rects.
fn live_bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 2560,
        h: 1380,
    }
}
fn live_domain(workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: live_bounds(),
        gap: 8,
        adjacent: BTreeMap::new(),
    }
}
fn live_hints(window: &str) -> crate::size_hints::WindowSizeHints {
    let (min_w, min_h) = match window {
        "notepad" => (401, 246),
        "paint" => (864, 617),
        "terminal" => (582, 95),
        _ => return crate::size_hints::WindowSizeHints::none(),
    };
    crate::size_hints::WindowSizeHints {
        min_w: Some(min_w),
        min_h: Some(min_h),
        max_w: None,
        max_h: None,
    }
}
fn live_carried(window: &str, workspace: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: live_bounds(),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: live_hints(window),
    }
}

#[test]
fn send_return_after_send_away_splits_mru_tall_target() {
    use crate::directional::Axis;
    let ws1 = live_domain("ws-1");
    let ws4 = live_domain("ws-4");
    let mut layout = WorldLayout::new();
    // Ordinary open order builds focus [notepad, paint, terminal].
    for (rows, focused) in [
        (vec![live_carried("notepad", "ws-1")], "notepad"),
        (
            vec![
                live_carried("notepad", "ws-1"),
                live_carried("paint", "ws-1"),
            ],
            "paint",
        ),
        (
            vec![
                live_carried("notepad", "ws-1"),
                live_carried("paint", "ws-1"),
                live_carried("terminal", "ws-1"),
            ],
            "terminal",
        ),
    ] {
        reconcile_domain(&mut layout, &ws1, 8, rows, focused);
    }
    // Mover away ws-1 -> ws-4 (empty target). last_active[ws-1] keeps the
    // departed terminal leaf; the ws-1 focus stack prunes to [notepad, paint].
    let focus = WindowId("terminal".to_owned());
    let away_obs = vec![
        live_carried("notepad", "ws-1"),
        live_carried("paint", "ws-1"),
        live_carried("terminal", "ws-1"),
    ];
    let away_ctx = PairOperationContext {
        source_domain: &ws1,
        target_domain: &ws4,
        source_outer_gap: 8,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &away_obs,
        closed: &[],
    };
    layout
        .run_pair_send(&away_ctx, &focus, true, false)
        .expect("send-away must commit");
    assert_eq!(
        leaves_of(&layout, "mon-a", "ws-1"),
        vec!["notepad", "paint"],
        "send-away leaves two tall survivors on ws-1"
    );
    // Return with NO ws-1 reconcile in between: stale remembered terminal
    // must fall back to the MRU survivor (paint), splitting that tall leaf.
    let back_obs = vec![
        live_carried("terminal", "ws-4"),
        live_carried("notepad", "ws-1"),
        live_carried("paint", "ws-1"),
    ];
    let back_ctx = PairOperationContext {
        source_domain: &ws4,
        target_domain: &ws1,
        source_outer_gap: 8,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &back_obs,
        closed: &[],
    };
    let geometry = layout
        .run_pair_send(&back_ctx, &focus, true, false)
        .expect("send-back must commit")
        .geometry;
    // Native assignment plus source removal.
    assert!(
        geometry
            .iter()
            .any(|g| g.window.0 == "terminal" && g.output.0 == "mon-a" && g.workspace.0 == "ws-1"),
        "return must carry the native assignment, got {geometry:?}"
    );
    assert!(
        !layout.contains_domain(&key("mon-a", "ws-4")),
        "ws-4 retires empty"
    );
    // Nested V topology splitting the MRU target: H[notepad V[paint terminal]].
    match layout.tree(&key("mon-a", "ws-1")) {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(*axis, Axis::Horizontal, "ws-1 root stays width-split");
            assert_eq!(children.len(), 2);
            assert!(
                matches!(&children[0], Node::Leaf { id } if *id == leaf_of(&layout, "mon-a", "ws-1", "notepad")),
                "left leaf stays notepad, got {:?}",
                children[0]
            );
            match &children[1] {
                Node::Group {
                    axis,
                    children: inner,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Vertical, "tall paint target stacks arrival");
                    assert_eq!(inner.len(), 2);
                    assert!(
                        matches!(&inner[0], Node::Leaf { id } if *id == leaf_of(&layout, "mon-a", "ws-1", "paint")),
                        "split target is MRU paint, got {inner:?}"
                    );
                    assert!(
                        matches!(&inner[1], Node::Leaf { id } if *id == leaf_of(&layout, "mon-a", "ws-1", "terminal")),
                        "arrival follows paint, got {inner:?}"
                    );
                }
                other => panic!("expected nested V[paint terminal], got {other:?}"),
            }
        }
        other => panic!("expected H[notepad V[paint terminal]], got {other:?}"),
    }
    // Physical feasibility with actual minima: contained, minima met, and
    // exact column sharing (which already implies no overlap for this
    // topology, so no generic overlap loop).
    let b = live_bounds();
    let mut by_window = BTreeMap::new();
    for g in &geometry {
        if g.workspace.0 == "ws-1" {
            by_window.insert(g.window.0.clone(), g.rect);
        }
    }
    assert_eq!(
        by_window.len(),
        3,
        "ws-1 geometry covers all three, got {geometry:?}"
    );
    for (w, r) in &by_window {
        assert!(r.w > 0 && r.h > 0, "{w} positive, got {r:?}");
        assert!(
            r.x >= b.x && r.y >= b.y && r.x + r.w <= b.x + b.w && r.y + r.h <= b.y + b.h,
            "{w} contained in {b:?}, got {r:?}"
        );
        let (mw, mh) = match w.as_str() {
            "notepad" => (401, 246),
            "paint" => (864, 617),
            "terminal" => (582, 95),
            _ => (1, 1),
        };
        assert!(r.w >= mw && r.h >= mh, "{w} meets min {mw}x{mh}, got {r:?}");
    }
    let (n, p, t) = (
        by_window["notepad"],
        by_window["paint"],
        by_window["terminal"],
    );
    assert_eq!((p.x, p.w), (t.x, t.w), "paint/terminal share one column");
    assert_eq!(p.y + p.h + 8, t.y, "paint stacks above terminal with gap 8");
    assert_eq!(n.x + n.w + 8, p.x, "notepad sits left of the paint column");
    let committed_tree = layout.tree(&key("mon-a", "ws-1")).cloned();
    // Stable subsequent reconciliation preserves the repaired topology.
    reconcile_domain(
        &mut layout,
        &ws1,
        8,
        vec![
            live_carried("notepad", "ws-1"),
            live_carried("paint", "ws-1"),
            live_carried("terminal", "ws-1"),
        ],
        "terminal",
    );
    assert_eq!(layout.tree(&key("mon-a", "ws-1")).cloned(), committed_tree);
    assert_eq!(
        leaves_of(&layout, "mon-a", "ws-1"),
        vec!["notepad", "paint", "terminal"]
    );
}

// ---- explicit output send (REQ-OUT-04, item 5.3/5.4) ----

fn seed_output_send_layout() -> (WorldLayout, OutputDomain, OutputDomain, OutputDomain) {
    // Cross-output seeding: source out-1/ws-a (s1, mover s2), destination
    // out-2/ws-a (t1a, t1b, t2), third domain out-1/ws-b (b1).
    let mut layout = WorldLayout::new();
    let source_domain = domain("out-1", "ws-a");
    admit_to(&mut layout, &source_domain, "s1");
    admit_to(&mut layout, &source_domain, "s2");
    let target_domain = domain("out-2", "ws-a");
    admit_to(&mut layout, &target_domain, "t1a");
    admit_to(&mut layout, &target_domain, "t1b");
    admit_to(&mut layout, &target_domain, "t2");
    let third_domain = domain("out-1", "ws-b");
    admit_to(&mut layout, &third_domain, "b1");
    (layout, source_domain, target_domain, third_domain)
}

fn parent_of(tree: &Node, leaf: &str) -> Option<String> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            if children.iter().any(|c| c.id().0 == leaf) {
                return Some(id.0.clone());
            }
            children.iter().find_map(|c| parent_of(c, leaf))
        }
    }
}

#[test]
fn output_send_uses_focus_history_when_remembered_stale() {
    let (mut layout, source_domain, target_domain, third_domain) = seed_output_send_layout();
    // Stage the stale anchor: send t2 (the destination's remembered leaf) to
    // ws-b, leaving last-active pointing at a departed leaf while the
    // destination focus history retains t1b.
    let stage_focus = WindowId("t2".to_owned());
    let stage_obs = vec![
        carried("t1a", "out-2", "ws-a", 0),
        carried("t1b", "out-2", "ws-a", 10),
        carried("t2", "out-2", "ws-a", 20),
        carried("b1", "out-1", "ws-b", 0),
    ];
    let stage_ctx = PairOperationContext {
        source_domain: &target_domain,
        target_domain: &third_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &stage_focus,
        observed: &stage_obs,
        closed: &[],
    };
    layout
        .run_pair_send(&stage_ctx, &stage_focus, true, true)
        .expect("staging send must commit");
    // Ordinary send of s2 onto out-2/ws-a: remembered t2 is stale, so the
    // destination focus history (t1b) anchors admission beside t1b.
    let focus = WindowId("s2".to_owned());
    let obs = vec![
        carried("s1", "out-1", "ws-a", 0),
        carried("s2", "out-1", "ws-a", 10),
        carried("t1a", "out-2", "ws-a", 0),
        carried("t1b", "out-2", "ws-a", 10),
    ];
    let ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &target_domain,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &obs,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, true, true)
        .expect("output send must commit");
    assert!(outcome.follow);
    assert_eq!(outcome.target_output.0, "out-2");
    assert_eq!(outcome.target_workspace.0, "ws-a");
    assert_eq!(
        outcome.focus_leaf.as_ref(),
        Some(&leaf_of(&layout, "out-2", "ws-a", "s2"))
    );
    let tree = layout
        .tree(&key("out-2", "ws-a"))
        .expect("target tree")
        .clone();
    assert_eq!(leaves_of(&layout, "out-2", "ws-a").len(), 3);
    assert_eq!(
        parent_of(&tree, &leaf_of(&layout, "out-2", "ws-a", "s2").0),
        parent_of(&tree, &leaf_of(&layout, "out-2", "ws-a", "t1b").0),
        "mover splits beside the focus-history leaf"
    );
    assert!(homed_in(&layout, "out-2", "ws-a", "s2"));
    // Both-domain geometry; the earlier send domain is undisturbed.
    let mut ids: Vec<(String, String)> = outcome
        .geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.output.0.clone()))
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            ("s1".to_owned(), "out-1".to_owned()),
            ("s2".to_owned(), "out-2".to_owned()),
            ("t1a".to_owned(), "out-2".to_owned()),
            ("t1b".to_owned(), "out-2".to_owned()),
        ]
    );
    let mut ws_b = leaves_of(&layout, "out-1", "ws-b");
    ws_b.sort();
    assert_eq!(ws_b, vec!["b1".to_string(), "t2".to_string()]);
}

#[test]
fn output_send_empty_destination_admits_lone_root_follow_and_stay() {
    // Genuine root fallback through WorldLayout: the destination output was
    // never observed, so the mover becomes its lone root with ordinary
    // follow/stay focus. Stay with a sole source window leaves no focus and
    // retires the emptied source slot.
    let mut layout = WorldLayout::new();
    let source_domain = domain("out-1", "ws-a");
    admit_to(&mut layout, &source_domain, "s1");
    admit_to(&mut layout, &source_domain, "m");
    let fresh_target = domain("out-2", "ws-1");
    let follow_focus = WindowId("m".to_owned());
    let follow_obs = vec![
        carried("s1", "out-1", "ws-a", 0),
        carried("m", "out-1", "ws-a", 10),
    ];
    let follow_ctx = PairOperationContext {
        source_domain: &source_domain,
        target_domain: &fresh_target,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &follow_focus,
        observed: &follow_obs,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&follow_ctx, &follow_focus, true, true)
        .expect("follow must commit");
    assert!(layout.contains_domain(&key("out-2", "ws-1")));
    assert_eq!(leaves_of(&layout, "out-2", "ws-1"), vec!["m".to_string()]);
    assert_eq!(outcome.target_output.0, "out-2");
    assert_eq!(outcome.target_workspace.0, "ws-1");
    assert_eq!(
        outcome.focus_leaf.as_ref(),
        Some(&leaf_of(&layout, "out-2", "ws-1", "m"))
    );
    let mut ids: Vec<(String, String)> = outcome
        .geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.output.0.clone()))
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            ("m".to_owned(), "out-2".to_owned()),
            ("s1".to_owned(), "out-1".to_owned()),
        ]
    );

    let mut solo_layout = WorldLayout::new();
    let solo_domain = domain("out-1", "ws-a");
    admit_to(&mut solo_layout, &solo_domain, "solo");
    let stay_focus = WindowId("solo".to_owned());
    let stay_obs = vec![carried("solo", "out-1", "ws-a", 0)];
    let stay_target = domain("out-2", "ws-1");
    let stay_ctx = PairOperationContext {
        source_domain: &solo_domain,
        target_domain: &stay_target,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &stay_focus,
        observed: &stay_obs,
        closed: &[],
    };
    let stay_outcome = solo_layout
        .run_pair_send(&stay_ctx, &stay_focus, false, true)
        .expect("stay must commit");
    assert!(!stay_outcome.follow);
    assert_eq!(stay_outcome.stay_focus_window, None);
    assert_eq!(stay_outcome.focus_leaf, None);
    assert!(solo_layout.contains_domain(&key("out-2", "ws-1")));
    assert_eq!(
        leaves_of(&solo_layout, "out-2", "ws-1"),
        vec!["solo".to_string()]
    );
    assert_eq!(stay_outcome.geometry.len(), 1);
    assert_eq!(stay_outcome.geometry[0].window.0, "solo");
    assert!(
        !solo_layout.contains_domain(&key("out-1", "ws-a")),
        "emptied source slot retires"
    );
}
