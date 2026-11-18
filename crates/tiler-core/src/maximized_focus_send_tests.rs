#![forbid(unsafe_code)]
//! Shared maximized directional focus local-skip/cross semantics + G-D2
//! maximized workspace-send carry (portable, headless).
//!
//! Drives the real `WorldLayout` focus/move/send APIs. A maximized
//! (non-fullscreen) focused subject skips same-output candidates
//! (`local_focus` refuses as `Unchanged`) and crosses to the adjacent
//! output when one exists; fullscreen keeps its enter/leave behavior.
//! G-D2: a maximized tiled mover sends to another workspace through the
//! ordinary transfer with its leaf preserved and follow/stay focus intact
//! (no unmaximize/remaximize in core).

use crate::directional::{Direction, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind};
use crate::world_layout::{
    LayoutFailure, PairOperationContext, PairOperationFailure, PairSendKind, WorldLayout,
};
use std::collections::BTreeMap;

fn dom(o: &str, w: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn dom_adjacent(o: &str, w: &str, dir: Direction, target: &str) -> OutputDomain {
    let mut d = dom(o, w);
    d.adjacent.insert(dir, OutputId(target.to_owned()));
    d
}
fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn engine_row(w: &str, o: &str, ws: &str, rect: Rect) -> EngineWindow {
    EngineWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect,
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
/// Complete observation over the retained membership with `overrides`
/// applied per window id (used to flag the focused mover maximized).
fn obs_for(
    layout: &WorldLayout,
    domain: &DomainKey,
    overrides: &BTreeMap<String, ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut wins: Vec<ObservedWindow> = layout
        .tiled_windows(domain)
        .map(|w| {
            if let Some(entry) = overrides.get(&w.0) {
                entry.clone()
            } else {
                tiled(&w.0, &domain.output.0, &domain.workspace.0)
            }
        })
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut entry = tiled(&w.0, &domain.output.0, &domain.workspace.0);
        entry.floating = flags.floating;
        entry.fullscreen = flags.fullscreen;
        entry.maximized = flags.maximized;
        entry.sticky = flags.sticky;
        entry.fixed_auto = layout.is_automatic_fixed_float(domain, &w);
        wins.push(entry);
    }
    wins
}
fn engine_obs_for_pair(
    layout: &WorldLayout,
    k1: &DomainKey,
    k2: &DomainKey,
    overrides: &BTreeMap<String, ObservedWindow>,
) -> Vec<EngineWindow> {
    let mut wins = Vec::new();
    for k in [k1, k2] {
        for o in obs_for(layout, k, overrides) {
            let mut row = engine_row(
                &o.window.0,
                &o.output.0,
                &o.workspace.0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            );
            row.floating = o.floating;
            row.fullscreen = o.fullscreen;
            row.maximized = o.maximized;
            row.sticky = o.sticky;
            row.fixed_auto = o.fixed_auto;
            wins.push(row);
        }
    }
    wins
}
fn maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.maximized = true;
    entry
}
fn fullscreen_maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = true;
    entry.maximized = true;
    entry
}
fn linux_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    layout
}
fn admit(layout: &mut WorldLayout, d: &OutputDomain, w: &str) {
    let k = d.key();
    let win_id = WindowId(w.to_owned());
    let placement = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    if !layout.contains_domain(&k) {
        layout
            .reconcile_fresh(
                d,
                0,
                &win_id,
                &[engine_row(w, &d.id.0, &d.workspace.0, placement)],
            )
            .expect("fresh admit");
        return;
    }
    let mut o0 = obs_for(layout, &k, &BTreeMap::new());
    o0.push(tiled(w, &d.id.0, &d.workspace.0));
    layout
        .admit_window(
            &k,
            &win_id,
            ExceptionFlags::none(),
            None,
            placement,
            false,
            false,
            &o0,
        )
        .expect("admit");
}
fn focused_window(layout: &WorldLayout, domain: &DomainKey) -> WindowId {
    let l = layout.focus_leaf(domain).expect("focused leaf");
    layout.window_for_leaf(domain, l).expect("focused link")
}

/// Two-tile layout focused on win-2 (win-1 left of win-2).
fn two_tile() -> (WorldLayout, OutputDomain, DomainKey) {
    let mut layout = linux_layout();
    let d = dom("out-1", "ws-1");
    admit(&mut layout, &d, "win-1");
    admit(&mut layout, &d, "win-2");
    let domain = d.key();
    assert_eq!(focused_window(&layout, &domain).0, "win-2");
    (layout, d, domain)
}

#[test]
fn maximized_local_focus_skips_sibling_with_no_pending() {
    let (mut layout, _, domain) = two_tile();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    let rev_before = layout.accepted_revision(&domain);
    let err = layout
        .local_focus(&domain, &focused, Direction::Left, &focused, &o0)
        .expect_err("maximized focus must skip same-output siblings");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    assert_eq!(layout.accepted_revision(&domain), rev_before);
}

#[test]
fn fullscreen_skips_local_sibling_while_ordinary_focus_plans() {
    let (mut layout, _, domain) = two_tile();
    let focused = focused_window(&layout, &domain);
    // Fullscreen (even with maximize also set) skips same-output siblings
    // (R-MAX-02): `local_focus` refuses as `Unchanged`.
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        fullscreen_maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    let rev_before = layout.accepted_revision(&domain);
    let err = layout
        .local_focus(&domain, &focused, Direction::Left, &focused, &o0)
        .expect_err("fullscreen focus skips the local sibling");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    assert_eq!(layout.accepted_revision(&domain), rev_before);
    // Ordinary focus still plans.
    let o1 = obs_for(&layout, &domain, &BTreeMap::new());
    layout
        .local_focus(&domain, &focused, Direction::Left, &focused, &o1)
        .expect("ordinary focus still plans");
}

#[test]
fn default_session_skips_maximized_local_focus() {
    // Shared maximized local-skip semantics apply without any opt-in flag.
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    let d = dom("out-1", "ws-1");
    admit(&mut layout, &d, "win-1");
    admit(&mut layout, &d, "win-2");
    let domain = d.key();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    let err = layout
        .local_focus(&domain, &focused, Direction::Left, &focused, &o0)
        .expect_err("default layout skips maximized local focus");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
}

#[test]
fn maximized_cross_output_focus_crosses_and_ordinary_crosses() {
    for (dir, opp) in [
        (Direction::Right, Direction::Left),
        (Direction::Up, Direction::Down),
        (Direction::Down, Direction::Up),
    ] {
        let setup = || {
            let mut out_2 = dom_adjacent("out-2", "ws-1", opp, "out-1");
            out_2.bounds.y = match dir {
                Direction::Up => -600,
                Direction::Down => 600,
                _ => 0,
            };
            let out_1 = dom_adjacent("out-1", "ws-1", dir, "out-2");
            let mut layout = linux_layout();
            admit(&mut layout, &out_2, "win-t");
            admit(&mut layout, &out_1, "win-a");
            let source = out_1.key();
            assert_eq!(focused_window(&layout, &source).0, "win-a");
            (layout, out_1, out_2, source)
        };
        let (mut layout, out_1, out_2, source) = setup();
        let target = out_2.key();
        let mut flags = BTreeMap::new();
        flags.insert(
            "win-a".to_owned(),
            maximized_flag(&tiled("win-a", "out-1", "ws-1")),
        );
        let o1 = engine_obs_for_pair(&layout, &source, &target, &flags);
        let subject = WindowId("win-a".to_owned());
        let ctx = PairOperationContext {
            source_domain: &out_1,
            target_domain: &out_2,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subject,
            observed: &o1,
            closed: &[],
        };
        let max_outcome = layout
            .run_directional_pair_focus(&ctx, &subject, dir, false)
            .expect("maximized subject crosses to adjacent output");
        assert!(max_outcome.crossed);
        assert_eq!(max_outcome.target_output, OutputId("out-2".to_owned()));
        assert_eq!(max_outcome.target_workspace, WorkspaceId("ws-1".to_owned()));
        // Control: ordinary cross-output focus crosses horizontally (`Right`),
        // while vertical ordinary cross-output focus stays `Unchanged`.
        let (mut layout_plain, out_1_p, out_2_p, source_plain) = setup();
        let target_plain = out_2_p.key();
        let o0 = engine_obs_for_pair(
            &layout_plain,
            &source_plain,
            &target_plain,
            &BTreeMap::new(),
        );
        let ctx_plain = PairOperationContext {
            source_domain: &out_1_p,
            target_domain: &out_2_p,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subject,
            observed: &o0,
            closed: &[],
        };
        match dir {
            Direction::Right => {
                let plain_outcome = layout_plain
                    .run_directional_pair_focus(&ctx_plain, &subject, dir, false)
                    .expect("ordinary cross-output focus plans");
                assert!(plain_outcome.crossed);
                assert_eq!(plain_outcome.target_output, OutputId("out-2".to_owned()));
                assert_eq!(
                    plain_outcome.target_workspace,
                    WorkspaceId("ws-1".to_owned())
                );
            }
            Direction::Up | Direction::Down => {
                let rev_before = layout_plain.accepted_revision(&source_plain);
                let err = layout_plain
                    .run_directional_pair_focus(&ctx_plain, &subject, dir, false)
                    .expect_err("vertical ordinary cross-output focus refuses unchanged");
                assert_eq!(
                    err,
                    PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
                );
                assert_eq!(layout_plain.accepted_revision(&source_plain), rev_before);
            }
            Direction::Left => unreachable!("test only covers Right/Up/Down"),
        }
    }
}

#[test]
fn maximized_move_plans_untouched_for_the_adapter_clear() {
    // Core never clears maximize itself: a maximized mover still
    // plans structurally, so the adapter's unmaximize-first leg lands
    // on the ordinary move once settlement is observed clear.
    let (mut layout, _, domain) = two_tile();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    layout
        .local_move(
            &domain,
            &focused,
            Direction::Left,
            crate::directional::SameAxisMove::GroupWithNeighbor,
            &focused,
            &o0,
        )
        .expect("move still plans with shared maximized focus semantics");
}

// ---- Nov 1 rule (2026-11-01): a maximized origin skips same-output
// candidates, crosses to the adjacent output in the requested direction,
// else stays. Shared with the native and composed maximize routes; the
// cross leg preserves the destination overlay. ----

fn plain_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    layout
}

/// Two tiles with no fence profile: focused win-2 with win-1 to the left.
fn unfenced_two_tile() -> (WorldLayout, OutputDomain, DomainKey) {
    let mut layout = plain_layout();
    let d = dom("out-1", "ws-1");
    admit(&mut layout, &d, "win-1");
    admit(&mut layout, &d, "win-2");
    let domain = d.key();
    assert_eq!(focused_window(&layout, &domain).0, "win-2");
    (layout, d, domain)
}

#[test]
fn nov1_maximized_local_focus_skips_sibling_all_directions() {
    let (mut layout, _, domain) = unfenced_two_tile();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    let rev_before = layout.accepted_revision(&domain);
    // A local sibling waits in Left; Up/Down/Right have no local target
    // either. Every direction must refuse as `Unchanged`.
    for direction in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        let err = layout
            .local_focus(&domain, &focused, direction, &focused, &o0)
            .expect_err("maximized focus must skip same-output siblings");
        assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
        assert_eq!(layout.accepted_revision(&domain), rev_before);
    }
}

#[test]
fn nov1_maximized_crosses_to_adjacent_despite_local_sibling() {
    for (dir, opp) in [
        (Direction::Right, Direction::Left),
        (Direction::Up, Direction::Down),
        (Direction::Down, Direction::Up),
    ] {
        let mut out_2 = dom_adjacent("out-2", "ws-1", opp, "out-1");
        out_2.bounds.y = match dir {
            Direction::Up => -600,
            Direction::Down => 600,
            _ => 0,
        };
        let out_1 = dom_adjacent("out-1", "ws-1", dir, "out-2");
        let mut layout = plain_layout();
        admit(&mut layout, &out_2, "win-t");
        admit(&mut layout, &out_1, "win-a");
        admit(&mut layout, &out_1, "win-b");
        let source = out_1.key();
        let target = out_2.key();
        // Move logical focus onto win-a so a same-output sibling (win-b)
        // waits on the output.
        let plain = obs_for(&layout, &source, &BTreeMap::new());
        layout
            .local_focus(
                &source,
                &WindowId("win-b".to_owned()),
                Direction::Left,
                &WindowId("win-b".to_owned()),
                &plain,
            )
            .expect("seed focus plans");
        assert_eq!(focused_window(&layout, &source).0, "win-a");
        // Maximized win-a pressing dir must skip win-b and cross to win-t
        // even though the local edge is not exhausted.
        let mut flags = BTreeMap::new();
        flags.insert(
            "win-a".to_owned(),
            maximized_flag(&tiled("win-a", "out-1", "ws-1")),
        );
        let o_local = obs_for(&layout, &source, &flags);
        let err = layout
            .local_focus(
                &source,
                &WindowId("win-a".to_owned()),
                dir,
                &WindowId("win-a".to_owned()),
                &o_local,
            )
            .expect_err("maximized local focus skips sibling");
        assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
        let o_pair = engine_obs_for_pair(&layout, &source, &target, &flags);
        let subject = WindowId("win-a".to_owned());
        let ctx = PairOperationContext {
            source_domain: &out_1,
            target_domain: &out_2,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subject,
            observed: &o_pair,
            closed: &[],
        };
        let outcome = layout
            .run_directional_pair_focus(&ctx, &subject, dir, false)
            .expect("maximized subject crosses despite the local sibling");
        assert!(outcome.crossed);
        assert_eq!(outcome.target_output, OutputId("out-2".to_owned()));
        assert_eq!(outcome.target_workspace, WorkspaceId("ws-1".to_owned()));
    }
}

#[test]
fn nov1_maximized_single_output_cross_stays() {
    let (mut layout, out_1, domain) = unfenced_two_tile();
    let out_2 = dom_adjacent("out-2", "ws-1", Direction::Left, "out-1");
    let target = out_2.key();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = engine_obs_for_pair(&layout, &domain, &target, &flags);
    let ctx = PairOperationContext {
        source_domain: &out_1,
        target_domain: &out_2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focused,
        observed: &o0,
        closed: &[],
    };
    let rev_before = layout.accepted_revision(&domain);
    let err = layout
        .run_directional_pair_focus(&ctx, &focused, Direction::Right, false)
        .expect_err("no adjacent output stays");
    assert!(matches!(
        err,
        PairOperationFailure::Assembly(_)
            | PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
    ));
    assert_eq!(layout.accepted_revision(&domain), rev_before);
}

#[test]
fn nov1_maximized_no_vertical_neighbor_cross_stays() {
    let mut layout = plain_layout();
    let out_1 = dom_adjacent("out-1", "ws-1", Direction::Right, "out-2");
    let out_2 = dom_adjacent("out-2", "ws-1", Direction::Left, "out-1");
    admit(&mut layout, &out_2, "win-t");
    admit(&mut layout, &out_1, "win-a");
    let source = out_1.key();
    let target = out_2.key();
    assert_eq!(focused_window(&layout, &source).0, "win-a");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-a".to_owned(),
        maximized_flag(&tiled("win-a", "out-1", "ws-1")),
    );
    let o1 = engine_obs_for_pair(&layout, &source, &target, &flags);
    let subject = WindowId("win-a".to_owned());
    for dir in [Direction::Up, Direction::Down] {
        let ctx = PairOperationContext {
            source_domain: &out_1,
            target_domain: &out_2,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subject,
            observed: &o1,
            closed: &[],
        };
        let rev_before = layout.accepted_revision(&source);
        let err = layout
            .run_directional_pair_focus(&ctx, &subject, dir, false)
            .expect_err("no vertical neighbor stays");
        assert_eq!(
            err,
            PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
        );
        assert_eq!(layout.accepted_revision(&source), rev_before);
    }
}

/// Occupied-target layout focused on win-2 (win-t waits on ws-b).
fn send_fixture() -> (WorldLayout, OutputDomain, OutputDomain) {
    let mut layout = linux_layout();
    let d_a = dom("out-1", "ws-a");
    let d_b = dom("out-1", "ws-b");
    admit(&mut layout, &d_b, "win-t");
    admit(&mut layout, &d_a, "win-1");
    admit(&mut layout, &d_a, "win-2");
    let source = d_a.key();
    assert_eq!(focused_window(&layout, &source).0, "win-2");
    (layout, d_a, d_b)
}

fn mover_leaf(
    layout: &WorldLayout,
    domain: &DomainKey,
    window: &str,
) -> crate::directional::NodeId {
    layout
        .bindings(domain)
        .find(|(w, _)| w.0 == window)
        .expect("mover link")
        .1
        .clone()
}

#[test]
fn maximized_mover_sends_with_leaf_preserved_and_follow_focus() {
    let (mut layout, d_a, d_b) = send_fixture();
    let source = d_a.key();
    let target = d_b.key();
    let leaf = mover_leaf(&layout, &source, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        maximized_flag(&tiled("win-2", "out-1", "ws-a")),
    );
    let o0 = engine_obs_for_pair(&layout, &source, &target, &flags);
    let subject = WindowId("win-2".to_owned());
    let ctx = PairOperationContext {
        source_domain: &d_a,
        target_domain: &d_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &o0,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subject, true, false)
        .expect("maximized send plans the ordinary transfer");
    // The whole window (leaf identity) relocates; core performs no
    // unmaximize/remaximize.
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(outcome.window.0, "win-2");
    assert_eq!(outcome.target_output, target.output);
    assert_eq!(outcome.target_workspace, target.workspace);
    assert_eq!(
        mover_leaf(&layout, &target, "win-2"),
        leaf,
        "mover keeps its leaf identity across the send"
    );
    assert_eq!(
        layout.focus_leaf(&target).cloned(),
        Some(leaf.clone()),
        "follow moves focus into the target and keeps the mover leaf"
    );
    assert_eq!(outcome.focus_leaf, Some(leaf));
    assert!(
        outcome
            .geometry
            .iter()
            .any(|g| g.window.0 == "win-2" && g.workspace == target.workspace),
        "mover keeps a retained target allocation (overlay, never removed)"
    );
    assert!(
        outcome.geometry.iter().any(|g| g.window.0 == "win-t"),
        "occupied target geometry covers the existing tile"
    );
}

#[test]
fn maximized_mover_stay_falls_back_to_source_mru() {
    let (mut layout, d_a, d_b) = send_fixture();
    let source = d_a.key();
    let target = d_b.key();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        maximized_flag(&tiled("win-2", "out-1", "ws-a")),
    );
    let o0 = engine_obs_for_pair(&layout, &source, &target, &flags);
    let subject = WindowId("win-2".to_owned());
    let ctx = PairOperationContext {
        source_domain: &d_a,
        target_domain: &d_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &o0,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subject, false, false)
        .expect("maximized stay-send plans");
    assert!(!outcome.follow);
    let survivor = outcome.focus_leaf.clone().expect("stay survivor leaf");
    assert_eq!(
        layout.window_for_leaf(&source, &survivor),
        Some(WindowId("win-1".to_owned())),
        "stay falls back to the source MRU survivor"
    );
    assert_eq!(
        outcome.stay_focus_window,
        Some(WindowId("win-1".to_owned())),
        "stay reports the source MRU survivor"
    );
}

// ---- Real-WorldLayout request: shared maximized local-skip semantics and
// fullscreen local-skip behave consistently. ----

fn engine_window(id: &str, x: i32, maximized: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: Rect {
            x,
            y: 0,
            w: 100,
            h: 100,
        },
        floating: false,
        fit_excluded: maximized,
        fullscreen: false,
        maximized,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

fn seed_layout(fixed_size_admission: bool) -> (WorldLayout, OutputDomain) {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(fixed_size_admission);
    let d = dom("out-1", "ws-1");
    layout
        .reconcile_fresh(
            &d,
            0,
            &WindowId("win-2".to_owned()),
            &[
                engine_window("win-1", 0, false),
                engine_window("win-2", 200, false),
            ],
        )
        .expect("seed must project");
    (layout, d)
}

#[test]
fn engine_skips_maximized_local_focus_through_handle() {
    let (mut layout, d) = seed_layout(true);
    let domain = d.key();
    let obs = vec![
        tiled("win-1", "out-1", "ws-1"),
        maximized_flag(&tiled("win-2", "out-1", "ws-1")),
    ];
    let err = layout
        .local_focus(
            &domain,
            &WindowId("win-2".to_owned()),
            Direction::Left,
            &WindowId("win-2".to_owned()),
            &obs,
        )
        .expect_err("maximized local focus must skip");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::Unchanged),
        "maximized local skip refuses as unchanged"
    );
}

#[test]
fn engine_fullscreen_skips_local_while_ordinary_plans() {
    let (mut layout, d) = seed_layout(true);
    let domain = d.key();
    let ordinary = vec![
        tiled("win-1", "out-1", "ws-1"),
        tiled("win-2", "out-1", "ws-1"),
    ];
    let mut layout_plain = layout.clone();
    layout_plain
        .local_focus(
            &domain,
            &WindowId("win-2".to_owned()),
            Direction::Left,
            &WindowId("win-2".to_owned()),
            &ordinary,
        )
        .expect("ordinary focus must plan");
    // Fullscreen skips the local sibling (R-MAX-02): the local request
    // refuses as unchanged; the cross leg continues instead.
    let mut full = maximized_flag(&tiled("win-2", "out-1", "ws-1"));
    full.fullscreen = true;
    let obs = vec![tiled("win-1", "out-1", "ws-1"), full];
    let err = layout
        .local_focus(
            &domain,
            &WindowId("win-2".to_owned()),
            Direction::Left,
            &WindowId("win-2".to_owned()),
            &obs,
        )
        .expect_err("fullscreen focus must skip the local sibling");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::Unchanged),
        "fullscreen skips local"
    );
}
