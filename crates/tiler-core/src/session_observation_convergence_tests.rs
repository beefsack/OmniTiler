#![forbid(unsafe_code)]
//! Complete-observation convergence rows.
//!
//! Design: docs/changes/archive/observation-convergence.md (2026-09-25).
//! [`WorldLayout`] converges retained membership/floating state to the complete
//! observation before running the ordinary operation, preserving revision
//! semantics, survivor topology, and typed rejections.

use std::collections::BTreeMap;

use crate::contract::ResizeMode;
use crate::directional::{Direction, Node, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::world_layout::{
    LayoutFailure, LocalOperationContext, LocalOperationFailure, PairOperationContext, WorldLayout,
};

fn domain(output: &str, workspace: &str, w: i32, h: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}
fn plain_obs(window: WindowId, output: OutputId, workspace: WorkspaceId) -> ObservedWindow {
    ObservedWindow {
        window,
        output,
        workspace,
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn carried(window: &str, rect: Rect, floating: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect,
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn placement() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    }
}

fn admit_to(layout: &mut WorldLayout, d: &OutputDomain, window: &str) {
    let k = d.key();
    if !layout.contains_domain(&k) {
        let win_id = WindowId(window.to_owned());
        let obs = vec![plain_obs(win_id.clone(), d.id.clone(), d.workspace.clone())];
        layout
            .converge_fresh_domain(d, 0, &obs, Some(&win_id))
            .expect("seed fresh domain");
        return;
    }
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(&k)
        .map(|w| plain_obs(w.clone(), k.output.clone(), k.workspace.clone()))
        .collect();
    windows.push(plain_obs(
        WindowId(window.to_owned()),
        k.output.clone(),
        k.workspace.clone(),
    ));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    layout
        .admit_window(
            &k,
            &WindowId(window.to_owned()),
            placement(),
            false,
            false,
            &windows,
        )
        .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
}

fn seed_layout() -> (WorldLayout, DomainKey, OutputDomain, u64) {
    let mut layout = WorldLayout::new();
    let d = domain("out-1", "ws-1", 800, 600);
    let k = d.key();
    admit_to(&mut layout, &d, "win-a");
    admit_to(&mut layout, &d, "win-b");
    admit_to(&mut layout, &d, "win-c");
    let base = layout.accepted_revision(&k);
    let leaf_c = layout
        .leaves(&k)
        .and_then(|leaves| leaves.get(&WindowId("win-c".to_owned())))
        .expect("win-c leaf");
    assert_eq!(
        layout.focus_leaf(&k),
        Some(leaf_c),
        "seed focuses the last admission"
    );
    (layout, k, d, base)
}

fn leaves(layout: &WorldLayout, k: &DomainKey) -> Vec<String> {
    fn collect(
        node: &Node,
        by_leaf: &BTreeMap<crate::directional::NodeId, String>,
        out: &mut Vec<String>,
    ) {
        match node {
            Node::Leaf { id } => out.push(by_leaf.get(id).cloned().unwrap_or_else(|| id.0.clone())),
            Node::Group { children, .. } => {
                for c in children {
                    collect(c, by_leaf, out);
                }
            }
        }
    }
    let by_leaf: BTreeMap<crate::directional::NodeId, String> = layout
        .leaves(k)
        .map(|l| l.iter().map(|(w, id)| (id.clone(), w.0.clone())).collect())
        .unwrap_or_default();
    match layout.tree(k) {
        None => vec![],
        Some(tree) => {
            let mut out = vec![];
            collect(tree, &by_leaf, &mut out);
            out
        }
    }
}

#[test]
fn engine_focus_with_floating_skew_converges_before_op() {
    // Retained tiled win-a observed floating: WorldLayout converges (removes
    // the win-a leaf, retains a floating exception, advances revision by one)
    // and then runs the ordinary focus at the converged revision. Valid
    // command: focus win-c leftwards to win-b over the H survivors.
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = focus_windows(true);
    let focus = WindowId("win-c".to_owned());
    let preconverged = layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("converge before focus")
        .is_some();
    let ctx = LocalOperationContext {
        domain: &domain_state,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &focus,
        observed: &obs,
        preconverged,
    };
    let plan = layout
        .run_local_focus(&ctx, &focus, Direction::Left)
        .expect("converged focus must plan at base+1");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "ordinary op runs at the converged revision"
    );
    assert_eq!(plan.to_window.0, "win-b");
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert!(
        ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
        "geometry covers converged survivors {ids:?}"
    );
    assert!(
        !ids.contains(&"win-a".to_string()),
        "converged float absent from tiled geometry"
    );
    assert_eq!(
        layout.accepted_revision(&k),
        base + 2,
        "convergence (+1) then ordinary op commit (+1), never a reseed reset"
    );
    assert!(
        layout.is_exception(&k, &WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(&layout, &k).iter().all(|l| l != "win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_move_with_floating_skew_converges_before_op() {
    // Same skew as the focus row; ordinary R1 perpendicular move of the
    // focused survivor (win-c down).
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = focus_windows(true);
    let focus = WindowId("win-c".to_owned());
    let preconverged = layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("converge before move")
        .is_some();
    let ctx = LocalOperationContext {
        domain: &domain_state,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &focus,
        observed: &obs,
        preconverged,
    };
    let plan = layout
        .run_local_move(
            &ctx,
            &focus,
            Direction::Down,
            crate::directional::SameAxisMove::GroupWithNeighbor,
        )
        .expect("converged move must plan at base+1");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "ordinary op runs at the converged revision"
    );
    assert_eq!(plan.direction, Direction::Down);
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert!(
        ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
        "geometry covers converged survivors {ids:?}"
    );
    assert!(
        !ids.contains(&"win-a".to_string()),
        "converged float absent from tiled geometry"
    );
    assert_eq!(
        layout.accepted_revision(&k),
        base + 2,
        "convergence (+1) then ordinary op commit (+1), never a reseed reset"
    );
    assert!(
        layout.is_exception(&k, &WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(&layout, &k).iter().all(|l| l != "win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_resize_with_floating_skew_converges_before_op() {
    // Same skew; ordinary keyboard resize of the focused survivor (win-c
    // left outwards).
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = focus_windows(true);
    let focus = WindowId("win-c".to_owned());
    let preconverged = layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("converge before resize")
        .is_some();
    let ctx = LocalOperationContext {
        domain: &domain_state,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &focus,
        observed: &obs,
        preconverged,
    };
    let plan = layout
        .run_keyboard_resize(&ctx, &focus, Direction::Left, ResizeMode::Outwards, 0)
        .expect("converged resize must plan at base+1");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "ordinary op runs at the converged revision"
    );
    let leaf_c = layout
        .leaves(&k)
        .and_then(|leaves| leaves.get(&WindowId("win-c".to_owned())))
        .expect("win-c leaf");
    assert_eq!(&plan.focus_leaf, leaf_c, "focus retained on survivor");
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert!(
        ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
        "geometry covers converged survivors {ids:?}"
    );
    assert!(
        !ids.contains(&"win-a".to_string()),
        "converged float absent from tiled geometry"
    );
    assert_eq!(
        layout.accepted_revision(&k),
        base + 2,
        "convergence (+1) then ordinary op commit (+1), never a reseed reset"
    );
    assert!(
        layout.is_exception(&k, &WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(&layout, &k).iter().all(|l| l != "win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_reconcile_with_missing_member_converges() {
    // Missing tiled win-b: removed using the current post-removal observation;
    // survivor topology/focus preserved, revision advanced, geometry covers
    // the converged plan.
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = strip(&[("win-a", false), ("win-c", false)]);
    let focus = WindowId("win-c".to_owned());
    layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("converge succeeds");
    let plan = layout
        .reconcile_retained(
            &k,
            domain_state.bounds,
            domain_state.gap,
            0,
            &focus,
            &obs,
            &[],
        )
        .expect("reconcile succeeds")
        .expect("retained domain present");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "converged plan at resulting revision"
    );
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert!(
        ids.contains(&"win-a".to_string()) && ids.contains(&"win-c".to_string()),
        "geometry covers converged survivors {ids:?}"
    );
    assert!(
        !ids.contains(&"win-b".to_string()),
        "removed member absent from geometry"
    );
    assert!(plan.focus_leaf.is_some());
    assert_eq!(leaves(&layout, &k), vec!["win-a", "win-c"]);
}

#[test]
fn engine_reconcile_with_unexpected_new_member_admits() {
    // Unexpected new normal win-n: admitted through normal placement without
    // reseeding survivors; geometry covers the converged plan.
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = strip(&[
        ("win-a", false),
        ("win-b", false),
        ("win-c", false),
        ("win-n", false),
    ]);
    let focus = WindowId("win-c".to_owned());
    layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("converge succeeds");
    let plan = layout
        .reconcile_retained(
            &k,
            domain_state.bounds,
            domain_state.gap,
            0,
            &focus,
            &obs,
            &[],
        )
        .expect("reconcile succeeds")
        .expect("retained domain present");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "converged plan at resulting revision"
    );
    let mut ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["win-a", "win-b", "win-c", "win-n"]);
    assert!(plan.focus_leaf.is_some());
    assert!(
        layout.is_tiled_window(&k, &WindowId("win-a".to_owned())),
        "survivors not reseeded"
    );
}

#[test]
fn engine_focus_with_exact_match_keeps_revision_semantics() {
    // Exact-match observation: no convergence bump. The ordinary focus runs
    // at the retained revision and only its own commit advances it.
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = focus_windows(false);
    let focus = WindowId("win-c".to_owned());
    let ctx = LocalOperationContext {
        domain: &domain_state,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &focus,
        observed: &obs,
        preconverged: false,
    };
    let plan = layout
        .run_local_focus(&ctx, &focus, Direction::Left)
        .expect("exact-match focus must plan");
    assert_eq!(
        plan.base_revision, base,
        "exact-match runs the op at the retained revision"
    );
    assert_eq!(plan.to_window.0, "win-b");
    assert_eq!(
        layout.accepted_revision(&k),
        base + 1,
        "only the ordinary op commit advances the revision"
    );
}

#[test]
fn engine_converge_error_returns_typed_rejection_without_reset() {
    // Cross-homed observation (win-a on ws-2): typed `CrossDomainMismatch`
    // rejection, nothing advanced, layout stays usable for the next op.
    let (mut layout, k, domain_state, base) = seed_layout();
    let skewed = EngineWindow {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        rect: Rect {
            x: 0,
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
    };
    let obs_err = vec![
        skewed,
        carried(
            "win-b",
            Rect {
                x: 10,
                y: 0,
                w: 10,
                h: 10,
            },
            false,
        ),
        carried(
            "win-c",
            Rect {
                x: 20,
                y: 0,
                w: 10,
                h: 10,
            },
            false,
        ),
    ];
    let focus = WindowId("win-c".to_owned());
    let ctx = LocalOperationContext {
        domain: &domain_state,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &focus,
        observed: &obs_err,
        preconverged: false,
    };
    let err = layout
        .run_local_focus(&ctx, &focus, Direction::Left)
        .expect_err("cross-homed observation must reject");
    assert_eq!(
        err,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch)),
        "primitive error maps to its typed rejection"
    );
    assert_eq!(layout.accepted_revision(&k), base, "error advances nothing");
    assert_eq!(leaves(&layout, &k), vec!["win-a", "win-b", "win-c"]);
    // Follow-up exact observation still runs the ordinary op (no reseed).
    let exact_obs = focus_windows(false);
    let exact_ctx = LocalOperationContext {
        domain: &domain_state,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &focus,
        observed: &exact_obs,
        preconverged: false,
    };
    let plan = layout
        .run_local_focus(&exact_ctx, &focus, Direction::Left)
        .expect("layout must stay usable after error");
    assert_eq!(plan.base_revision, base);
    assert_eq!(plan.to_window.0, "win-b");
}

#[test]
fn engine_reconcile_with_floating_skew_converges() {
    // Retained tiled win-a observed floating: reconcile converges (leaf
    // removed, floating exception retained, revision +1) then projects the
    // converged survivors without reset.
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = focus_windows(true);
    let focus = WindowId("win-c".to_owned());
    layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("converge succeeds");
    let plan = layout
        .reconcile_retained(
            &k,
            domain_state.bounds,
            domain_state.gap,
            0,
            &focus,
            &obs,
            &[],
        )
        .expect("reconcile succeeds")
        .expect("retained domain present");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "converged plan at resulting revision"
    );
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert!(
        ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
        "geometry covers converged survivors {ids:?}"
    );
    assert!(
        !ids.contains(&"win-a".to_string()),
        "converged float absent from tiled geometry"
    );
    assert!(plan.focus_leaf.is_some());
    assert_eq!(layout.accepted_revision(&k), base + 1);
    assert!(
        layout.is_exception(&k, &WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(&layout, &k).iter().all(|l| l != "win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_fresh_mixed_float_and_tiled_reconciles() {
    // Fresh domain with a mixed floating/tiled observation: reconcile
    // converges the float exception plus the normal admission atomically,
    // returning tiled-only geometry at revision 1.
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    let domain_state = domain("out-1", "ws-1", 800, 600);
    let obs = strip(&[("win-f", true), ("win-n", false)]);
    let focus = WindowId("win-n".to_owned());
    let outcome = layout
        .reconcile_fresh(&domain_state, 0, &focus, &obs)
        .expect("fresh mixed reconcile must plan");
    let plan = outcome.projection;
    assert_eq!(
        plan.base_revision, 1,
        "fresh convergence (+1) at the converged revision"
    );
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert_eq!(
        ids,
        vec!["win-n".to_string()],
        "tiled-only geometry covers the admitted normal {ids:?}"
    );
    assert!(plan.focus_leaf.is_some());
    assert_eq!(layout.accepted_revision(&k), 1);
    assert!(
        layout.is_exception(&k, &WindowId("win-f".to_owned())),
        "float retained as exception, never tiled"
    );
    assert!(
        layout.is_tiled_window(&k, &WindowId("win-n".to_owned())),
        "normal admitted"
    );
    assert_eq!(leaves(&layout, &k), vec!["win-n"]);
}

fn carried_in(window: &str, output: &str, workspace: &str, x: i32, floating: bool) -> EngineWindow {
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
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

/// Focus-path carried set (membership by id; one layout shared by every
/// focus row so spatial order cannot drift between them).
fn focus_windows(floating_a: bool) -> Vec<EngineWindow> {
    vec![
        carried(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 600,
            },
            floating_a,
        ),
        carried(
            "win-b",
            Rect {
                x: 400,
                y: 0,
                w: 200,
                h: 600,
            },
            false,
        ),
        carried(
            "win-c",
            Rect {
                x: 600,
                y: 0,
                w: 200,
                h: 600,
            },
            false,
        ),
    ]
}

/// 10x10 carried windows in observation order for reconcile/admit/remove
/// rows (membership by id; observed rects are opaque carried frames).
fn strip(items: &[(&str, bool)]) -> Vec<EngineWindow> {
    items
        .iter()
        .enumerate()
        .map(|(i, (name, floating))| carried_in(name, "out-1", "ws-1", i as i32 * 10, *floating))
        .collect()
}

#[test]
fn engine_paired_focus_with_source_skew_converges() {
    // Paired focus with source membership skew (unexpected win-n): converges,
    // then the ordinary local focus proceeds with survivor topology intact.
    let mut layout = WorldLayout::new();
    let source_domain = domain("out-1", "ws-1", 800, 600);
    let target_domain = domain("out-2", "ws-1", 800, 600);
    admit_to(&mut layout, &source_domain, "win-a");
    admit_to(&mut layout, &source_domain, "win-b");
    admit_to(&mut layout, &target_domain, "win-t");
    let source_key = key("out-1", "ws-1");
    let target_key = key("out-2", "ws-1");
    // Pair domains carry reciprocal adjacency.
    let source_pair_domain = OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::from([(Direction::Right, OutputId("out-2".to_owned()))]),
    };
    let target_pair_domain = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::from([(Direction::Left, OutputId("out-1".to_owned()))]),
    };
    let observed = vec![
        carried_in("win-a", "out-1", "ws-1", 0, false),
        carried_in("win-b", "out-1", "ws-1", 0, false),
        carried_in("win-n", "out-1", "ws-1", 0, false),
        carried_in("win-t", "out-2", "ws-1", 0, false),
    ];
    let focus = WindowId("win-b".to_owned());
    let ctx = PairOperationContext {
        source_domain: &source_pair_domain,
        target_domain: &target_pair_domain,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let plan = layout
        .run_directional_pair_focus(&ctx, &focus, Direction::Left, false)
        .expect("converged pair focus must plan");
    let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
    assert!(
        ids.contains(&"win-a".to_string())
            && ids.contains(&"win-b".to_string())
            && ids.contains(&"win-n".to_string()),
        "geometry covers converged source survivors {ids:?}"
    );
    assert!(
        layout.is_tiled_window(&source_key, &WindowId("win-n".to_owned())),
        "skewed source member converged into retained topology"
    );
    let source_leaves = leaves(&layout, &source_key);
    assert!(
        source_leaves.contains(&"win-a".to_string())
            && source_leaves.contains(&"win-b".to_string()),
        "survivor topology persists without reseed {source_leaves:?}"
    );
    assert!(
        layout.is_tiled_window(&target_key, &WindowId("win-t".to_owned())),
        "target topology persists"
    );
}

#[test]
fn engine_reconcile_valid_exception_only_domain_projects_empty() {
    // Valid exception-only domain: every known member is a floating
    // exception and no tiled leaves remain. Reconcile with the complete
    // exception-only observation must return an empty projection, never
    // `malformed-topology`.
    let (mut layout, k, domain_state, base) = seed_layout();
    let obs = strip(&[("win-a", true), ("win-b", true), ("win-c", true)]);
    let focus = WindowId("win-c".to_owned());
    layout
        .converge_retained_observation(&k, &obs, &focus, &[])
        .expect("exception-only converge succeeds");
    let plan = layout
        .reconcile_retained(
            &k,
            domain_state.bounds,
            domain_state.gap,
            0,
            &focus,
            &obs,
            &[],
        )
        .expect("exception-only reconcile must project empty")
        .expect("retained domain present");
    assert_eq!(
        plan.base_revision,
        base + 1,
        "converged plan at resulting revision"
    );
    assert!(
        plan.geometry.is_empty(),
        "exception-only projects empty geometry, got {:?}",
        plan.geometry
    );
    assert!(
        plan.focus_leaf.is_none(),
        "exception-only carries no tiled focus"
    );
    assert_eq!(layout.accepted_revision(&k), base + 1);
    for name in ["win-a", "win-b", "win-c"] {
        assert!(
            layout.is_exception(&k, &WindowId(name.to_owned())),
            "{name} retained as floating exception"
        );
    }
    assert!(leaves(&layout, &k).is_empty(), "no tiled leaves remain");
}
