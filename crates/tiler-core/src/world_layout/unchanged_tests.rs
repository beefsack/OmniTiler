//! Direct tests for the conservative unchanged-domain proof. No oracle,
//! snapshots or harness: a hit is checked by the boolean plus the focus tail
//! it settles, and refusals are checked to leave retained state byte-identical.

use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags};
use crate::size_hints::WindowSizeHints;

use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn node(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn window(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn leaf(id: &str) -> Node {
    Node::Leaf { id: node(id) }
}

fn group(id: &str, leaves: &[&str]) -> Node {
    Node::Group {
        id: node(id),
        axis: Axis::Horizontal,
        children: leaves.iter().map(|l| leaf(l)).collect(),
        shares: vec![1; leaves.len()],
    }
}

fn observed_window(id: &str, output: &str, workspace: &str) -> EngineWindow {
    EngineWindow {
        window: window(id),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
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

/// One valid two-leaf-plus domain with scrambled (non-fixed-point) focus
/// bookkeeping, an untouched second domain, and the exact unhinted observation
/// the sweep would carry. Geometry is `(0,0,120,80)`, gap/outer-gap zero.
fn fixture() -> (
    WorldLayout,
    DomainKey,
    Rect,
    i32,
    i32,
    WindowId,
    Vec<EngineWindow>,
) {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let k = key("out", "ws");
    let mut retained = DomainLayout::new(bounds, 0, 0);
    retained.tree = Some(group("g", &["a", "b", "c"]));
    retained.leaves.insert(window("w-a"), node("a"));
    retained.leaves.insert(window("w-b"), node("b"));
    retained.leaves.insert(window("w-c"), node("c"));
    retained.focused_leaf = Some(node("b"));
    retained.focus_stack = vec![node("a"), node("stale"), node("c")];
    retained.focus_history = vec![node("a"), node("stale"), node("b")];
    retained.last_active = Some(node("stale"));
    retained.accepted_revision = 7;

    let other_key = key("out-2", "ws");
    let mut other = DomainLayout::new(bounds, 0, 0);
    other.tree = Some(leaf("z"));
    other.leaves.insert(window("w-z"), node("z"));
    other.focused_leaf = Some(node("z"));
    other.focus_stack = vec![node("z")];
    other.focus_history = vec![node("z")];
    other.last_active = Some(node("z"));
    other.accepted_revision = 3;

    let mut layout = WorldLayout::new();
    layout.domains.insert(k.clone(), retained);
    layout.domains.insert(other_key, other);
    let observed = vec![
        observed_window("w-a", "out", "ws"),
        observed_window("w-b", "out", "ws"),
        observed_window("w-c", "out", "ws"),
    ];
    (layout, k, bounds, 0, 0, window("w-b"), observed)
}

#[test]
fn unchanged_hit_settles_focus_tail_without_revision_or_highwater_bump() {
    let (mut layout, k, bounds, gap, outer_gap, focus_win, observed) = fixture();
    let other_key = key("out-2", "ws");
    let revision = layout.domains[&k].accepted_revision;
    let highwater = layout.admission_seq_next;
    let other_before = format!("{:?}", layout.domains[&other_key]);

    assert!(layout.try_unchanged_reconcile(&k, bounds, gap, outer_gap, &focus_win, &observed, &[]));

    let retained = &layout.domains[&k];
    assert_eq!(retained.focused_leaf, Some(node("b")));
    assert_eq!(
        retained.focus_stack,
        vec![node("a"), node("c"), node("b")],
        "stale leaf pruned and resolving focus demoted to MRU tail"
    );
    assert_eq!(
        retained.focus_history,
        vec![node("a"), node("b")],
        "stale history pruned without appending observed focus"
    );
    assert_eq!(retained.last_active, Some(node("b")));
    assert_eq!(
        retained.accepted_revision, revision,
        "hit is revision-neutral"
    );
    assert_eq!(
        layout.admission_seq_next, highwater,
        "hit is high-water-neutral"
    );
    assert_eq!(
        format!("{:?}", layout.domains[&other_key]),
        other_before,
        "another domain must be untouched"
    );

    // An empty observed focus is admissible and retains the resolving focus.
    let (mut empty, k, bounds, gap, outer_gap, _, observed) = fixture();
    assert!(empty.try_unchanged_reconcile(
        &k,
        bounds,
        gap,
        outer_gap,
        &WindowId(String::new()),
        &observed,
        &[]
    ));
    assert_eq!(empty.domains[&k].focused_leaf, Some(node("b")));
}

#[test]
fn unchanged_misses_are_conservative_and_do_not_mutate() {
    let (base, k, bounds, gap, outer_gap, focus_win, observed) = fixture();
    let miss = |layout: &mut WorldLayout,
                b: Rect,
                g: i32,
                og: i32,
                obs: &[EngineWindow],
                fw: &WindowId,
                closed: &[WindowId]| {
        let before = format!("{layout:?}");
        assert!(!layout.try_unchanged_reconcile(&k, b, g, og, fw, obs, closed));
        assert_eq!(format!("{layout:?}"), before, "miss must not mutate");
    };

    let mut layout = base.clone();
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap,
        &observed,
        &focus_win,
        &[window("w-a")],
    );
    miss(
        &mut layout,
        Rect {
            w: bounds.w + 1,
            ..bounds
        },
        gap,
        outer_gap,
        &observed,
        &focus_win,
        &[],
    );
    miss(
        &mut layout,
        bounds,
        gap + 1,
        outer_gap,
        &observed,
        &focus_win,
        &[],
    );
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap + 1,
        &observed,
        &focus_win,
        &[],
    );
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap,
        &observed,
        &window("w-a"),
        &[],
    );

    let flag_miss = |set: fn(&mut EngineWindow)| {
        let mut obs = observed.clone();
        set(&mut obs[0]);
        let mut layout = base.clone();
        miss(&mut layout, bounds, gap, outer_gap, &obs, &focus_win, &[]);
    };
    flag_miss(|w| w.floating = true);
    flag_miss(|w| w.fit_excluded = true);
    flag_miss(|w| w.fullscreen = true);
    flag_miss(|w| w.maximized = true);
    flag_miss(|w| w.sticky = true);
    flag_miss(|w| w.fixed_auto = true);
    flag_miss(|w| w.fixed_suppress = true);
    flag_miss(|w| {
        w.hints = WindowSizeHints {
            min_w: Some(1),
            ..WindowSizeHints::none()
        }
    });
    flag_miss(|w| w.output = OutputId("other".to_owned()));

    let mut extra = observed.clone();
    extra.push(observed_window("new", "out", "ws"));
    let mut layout = base.clone();
    miss(&mut layout, bounds, gap, outer_gap, &extra, &focus_win, &[]);
    let missing = &observed[..2];
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap,
        missing,
        &focus_win,
        &[],
    );

    let mark_miss = |change: fn(&mut DomainLayout)| {
        let mut layout = base.clone();
        change(layout.domains.get_mut(&k).expect("key domain"));
        miss(
            &mut layout,
            bounds,
            gap,
            outer_gap,
            &observed,
            &focus_win,
            &[],
        );
    };
    mark_miss(|d| {
        d.exceptions.insert(
            window("x"),
            DomainException {
                flags: ExceptionFlags::none(),
                floating_geometry: None,
            },
        );
    });
    mark_miss(|d| {
        d.minimized_slots.insert(
            window("x"),
            DomainRestoreSlot {
                leaf: node("x"),
                tree: leaf("x"),
            },
        );
    });
    mark_miss(|d| {
        d.automatic_fixed.insert(window("x"));
    });
    mark_miss(|d| {
        d.fixed_tile_override.insert(window("x"));
    });
    mark_miss(|d| {
        d.capacity_floats.insert(window("x"));
    });
    mark_miss(|d| {
        d.admission_order.insert(window("x"), 9);
    });
    mark_miss(|d| {
        d.focused_leaf = Some(node("stale"));
    });
}

#[test]
fn unchanged_hit_matches_reconcile_retained_state_and_focus_bookkeeping() {
    let (mut skipped, k, bounds, gap, outer_gap, focus_win, observed) = fixture();
    let mut converged = skipped.clone();

    assert!(skipped.try_unchanged_reconcile(
        &k,
        bounds,
        gap,
        outer_gap,
        &focus_win,
        &observed,
        &[]
    ));
    converged
        .converge_retained_observation(&k, &observed, &focus_win, &[])
        .expect("converge_retained_observation succeeds")
        .expect("retained domain present");

    let s = skipped.domain(&k).expect("skipped domain retained");
    let c = converged.domain(&k).expect("converged domain retained");
    assert_eq!(s.tree, c.tree);
    assert_eq!(s.leaves, c.leaves);
    assert_eq!(s.bounds, c.bounds);
    assert_eq!(s.gap, c.gap);
    assert_eq!(s.outer_gap, c.outer_gap);
    assert_eq!(s.focused_leaf, c.focused_leaf);
    assert_eq!(s.focus_stack, c.focus_stack);
    assert_eq!(s.focus_history, c.focus_history);
    assert_eq!(s.last_active, c.last_active);
    assert_eq!(s.exceptions.len(), c.exceptions.len());
    assert_eq!(s.retained_float_geometry, c.retained_float_geometry);
    assert_eq!(s.automatic_fixed, c.automatic_fixed);
    assert_eq!(s.fixed_tile_override, c.fixed_tile_override);
    assert_eq!(s.capacity_floats, c.capacity_floats);
    assert_eq!(s.admission_order, c.admission_order);
    assert_eq!(s.minimized_slots.len(), c.minimized_slots.len());
    assert_eq!(s.accepted_revision, c.accepted_revision);
    assert_eq!(skipped.admission_seq_next, converged.admission_seq_next);
    assert_eq!(
        skipped.project_domain_geometry(&k, &std::collections::BTreeMap::new()),
        converged.project_domain_geometry(&k, &std::collections::BTreeMap::new())
    );
}

#[test]
fn admission_highwater_survives_domain_retirement() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 300,
        h: 100,
    };
    let mut layout = WorldLayout::new();

    // Seed an active domain and a separate retiring domain, consuming real admission stamps.
    let active_key = key("out", "ws");
    let active_obs = vec![
        observed_window("w-a", "out", "ws"),
        observed_window("w-b", "out", "ws"),
    ];
    let (active_dom, c1) = layout
        .build_tiled_seed(
            &crate::session::OutputDomain {
                id: active_key.output.clone(),
                workspace: active_key.workspace.clone(),
                bounds,
                gap: 0,
                adjacent: std::collections::BTreeMap::new(),
            },
            0,
            &active_obs,
        )
        .expect("active domain seeds");
    layout.admission_seq_next = c1;
    layout.domains.insert(active_key.clone(), active_dom);

    let retiring_key = key("out-retire", "ws");
    let retiring_obs = vec![
        observed_window("w-r1", "out-retire", "ws"),
        observed_window("w-r2", "out-retire", "ws"),
    ];
    let (retiring_dom, c2) = layout
        .build_tiled_seed(
            &crate::session::OutputDomain {
                id: retiring_key.output.clone(),
                workspace: retiring_key.workspace.clone(),
                bounds,
                gap: 0,
                adjacent: std::collections::BTreeMap::new(),
            },
            0,
            &retiring_obs,
        )
        .expect("retiring domain seeds");
    layout.admission_seq_next = c2;
    let retired_max_stamp = *retiring_dom
        .admission_order
        .values()
        .max()
        .expect("retiring domain has admission stamps");
    layout.domains.insert(retiring_key.clone(), retiring_dom);
    assert!(layout.admission_seq_next > retired_max_stamp);

    // Retire the second domain via real close + empty gap update.
    let closed = [window("w-r1"), window("w-r2")];
    layout
        .converge_retained_observation(&retiring_key, &[], &WindowId(String::new()), &closed)
        .expect("converge close succeeds");
    layout
        .update_domain_gaps(&retiring_key, bounds, 0, 0, &WindowId(String::new()), &[])
        .expect("retiring empty domain succeeds");
    assert!(!layout.contains_domain(&retiring_key));
    let highwater_after_retire = layout.admission_seq_next;
    assert!(highwater_after_retire > retired_max_stamp);

    // Skip hit on the surviving domain preserves the high-water mark.
    assert!(layout.try_unchanged_reconcile(
        &active_key,
        bounds,
        0,
        0,
        &window("w-b"),
        &active_obs,
        &[],
    ));
    assert_eq!(layout.admission_seq_next, highwater_after_retire);

    // A subsequent admission on the surviving domain receives a strictly monotonic stamp.
    let mut next_obs = active_obs;
    next_obs.push(observed_window("w-c", "out", "ws"));
    layout
        .converge_retained_observation(&active_key, &next_obs, &window("w-c"), &[])
        .expect("new admission succeeds");
    let new_stamp = *layout
        .domain(&active_key)
        .expect("active domain retained")
        .admission_order
        .get(&window("w-c"))
        .expect("new window stamped");
    assert!(
        new_stamp > retired_max_stamp,
        "new stamp {new_stamp} must exceed retired max stamp {retired_max_stamp}"
    );
    assert!(layout.admission_seq_next > new_stamp);
}
