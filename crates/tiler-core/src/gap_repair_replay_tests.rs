//! Gap-repair discrepancy replay: visible staged shrink (8->32->64) against a
//! hidden-style direct jump (8->64) on matched retained preimages, plus a
//! reveal-route mirror (adopt gaps, then reconcile) and a shape-dependence
//! witness. Geometry below is generic representative stress input (numeric
//! regression values), not host detail.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::{Rect, project};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::size_hints::{WindowSizeHints, project_with_hints};
use crate::world_layout::{LayoutFailure, RetainedProjectionOutcome, WorldLayout};

const OUTPUT: &str = "display-0";
const WORKSPACE: &str = "ws-1";
const ROSTER: usize = 40;
const EXPECTED_AT_32: usize = 28;
const EXPECTED_AT_64: usize = 30;

fn work_area() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 2560,
        h: 1380,
    }
}

fn helper_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(118),
        min_h: Some(30),
        max_w: None,
        max_h: None,
    }
}

fn birth_rect() -> Rect {
    Rect {
        x: 200,
        y: 200,
        w: 640,
        h: 480,
    }
}

fn token(i: usize) -> WindowId {
    WindowId(format!("w{i:02}"))
}

/// Single shared row constructor: both replay branches carry identical
/// identities, rects, hints, and tiled flags by construction.
fn carried_rows() -> Vec<EngineWindow> {
    (0..ROSTER)
        .map(|i| EngineWindow {
            window: token(i),
            output: OutputId(OUTPUT.to_owned()),
            workspace: WorkspaceId(WORKSPACE.to_owned()),
            rect: birth_rect(),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: helper_hints(),
        })
        .collect()
}

fn domain_for(inner_gap: i32) -> (OutputDomain, DomainKey) {
    domain_for_output(OUTPUT, WORKSPACE, inner_gap)
}

fn domain_for_output(output: &str, workspace: &str, inner_gap: i32) -> (OutputDomain, DomainKey) {
    let bounds =
        crate::geometry::inset_bounds(work_area(), inner_gap).expect("positive inset bounds");
    let key = DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds,
        gap: inner_gap,
        adjacent: std::collections::BTreeMap::new(),
    };
    (domain, key)
}

#[derive(Clone, Copy)]
enum FocusRule {
    Newest,
    None,
    First,
}

impl FocusRule {
    fn label(self) -> &'static str {
        match self {
            FocusRule::Newest => "focus-newest",
            FocusRule::None => "focus-none",
            FocusRule::First => "focus-first",
        }
    }

    fn admission_focus(self, i: usize) -> Option<WindowId> {
        match self {
            FocusRule::Newest => Some(token(i)),
            FocusRule::None => None,
            FocusRule::First => Some(token(0)),
        }
    }
}

fn admit_roster(focus: FocusRule) -> WorldLayout {
    let mut layout = WorldLayout::new();
    let (domain, key) = domain_for(8);
    let all_rows = carried_rows();
    for i in 0..ROSTER {
        let carried = &all_rows[..=i];
        let focused = focus
            .admission_focus(i)
            .unwrap_or_else(|| WindowId(String::new()));
        layout
            .converge_retained_observation(&key, carried, &focused, &[])
            .expect("observation convergence succeeds");
        match layout
            .reconcile_retained(&key, domain.bounds, domain.gap, 8, &focused, carried, &[])
            .expect("retained reconcile succeeds")
        {
            Some(_) => {}
            None => {
                layout
                    .reconcile_fresh(&domain, 8, &focused, carried)
                    .expect("fresh reconcile succeeds");
            }
        }
    }
    assert_eq!(layout.tiled_count(&key), ROSTER);
    assert!(capacity_victims(&layout, &key).is_empty());
    layout
}

fn capacity_victims(layout: &WorldLayout, key: &DomainKey) -> BTreeSet<String> {
    (0..ROSTER)
        .map(token)
        .filter(|id| layout.is_capacity_float(key, id))
        .map(|id| id.0)
        .collect()
}

fn admission_sequences(layout: &WorldLayout, key: &DomainKey) -> BTreeMap<String, u64> {
    (0..ROSTER)
        .map(token)
        .map(|id| {
            let seq = layout.admission_sequence(key, &id).expect("tracked order");
            (id.0, seq)
        })
        .collect()
}

/// Newest-first victim suffix of length `n`, mirroring the repair's
/// newest-first victim order (untracked oldest, identity descending ties).
fn newest_suffix(sequences: &BTreeMap<String, u64>, n: usize) -> BTreeSet<String> {
    let mut ranked: Vec<(&String, &u64)> = sequences.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| b.0.cmp(a.0)));
    ranked
        .into_iter()
        .take(n)
        .map(|(id, _)| (*id).clone())
        .collect()
}

fn tiled_links(layout: &WorldLayout, key: &DomainKey) -> usize {
    layout.tiled_count(key)
}

fn retained_tree(layout: &WorldLayout, key: &DomainKey) -> Node {
    layout.tree(key).cloned().expect("retained tree")
}

fn update_gaps(
    layout: &mut WorldLayout,
    key: &DomainKey,
    gap: i32,
    focused: Option<&WindowId>,
) -> Result<RetainedProjectionOutcome, LayoutFailure> {
    let (domain, _) = domain_for(gap);
    let carried = carried_rows();
    let focus = focused.cloned().unwrap_or_else(|| WindowId(String::new()));
    layout.update_domain_gaps(key, domain.bounds, domain.gap, gap, &focus, &carried)
}

fn reconcile(
    layout: &mut WorldLayout,
    key: &DomainKey,
    gap: i32,
    focused: Option<&WindowId>,
) -> RetainedProjectionOutcome {
    let (domain, _) = domain_for(gap);
    let carried = carried_rows();
    let focus = focused.cloned().unwrap_or_else(|| WindowId(String::new()));
    layout
        .converge_retained_observation(key, &carried, &focus, &[])
        .expect("observation convergence succeeds");
    layout
        .reconcile_retained(key, domain.bounds, domain.gap, gap, &focus, &carried, &[])
        .expect("retained reconcile succeeds")
        .expect("retained domain present")
}

#[test]
fn matched_visible_vs_hidden_gap_repair() {
    assert!(
        carried_rows()
            .iter()
            .all(|row| row.hints == helper_hints() && !row.floating),
        "replay inputs must carry identical hints and tiled flags"
    );
    for focus in [FocusRule::Newest, FocusRule::None, FocusRule::First] {
        let mode = focus.label();
        let mut visible = admit_roster(focus);
        let mut hidden = admit_roster(focus);
        let (_, key) = domain_for(8);
        // Matched layout inputs: identical trees with shares, bindings, and admission order.
        assert_eq!(
            retained_tree(&visible, &key),
            retained_tree(&hidden, &key),
            "{mode}: retained trees must match before replay"
        );
        assert_eq!(
            visible
                .bindings(&key)
                .map(|(w, l)| (w.clone(), l.clone()))
                .collect::<Vec<_>>(),
            hidden
                .bindings(&key)
                .map(|(w, l)| (w.clone(), l.clone()))
                .collect::<Vec<_>>(),
            "{mode}: retained bindings must match before replay"
        );
        let order = admission_sequences(&visible, &key);
        assert_eq!(order.len(), ROSTER);
        assert_eq!(
            order,
            admission_sequences(&hidden, &key),
            "{mode}: admission order must match before replay"
        );
        let expected_28 = newest_suffix(&order, EXPECTED_AT_32);
        let expected_30 = newest_suffix(&order, EXPECTED_AT_64);
        assert_eq!(expected_28.len(), EXPECTED_AT_32);
        assert_eq!(expected_30.len(), EXPECTED_AT_64);

        let focus_of = |layout: &WorldLayout| -> Option<WindowId> {
            match focus {
                FocusRule::None => None,
                FocusRule::Newest => Some(token(ROSTER - 1)),
                FocusRule::First => Some(token(0)),
            }
            .filter(|id| layout.is_tiled_window(&key, id))
        };

        // Staged visible sequence: 8 -> 32 -> 64.
        let focus_val = focus_of(&visible);
        update_gaps(&mut visible, &key, 32, focus_val.as_ref()).expect("8->32 succeeds");
        let victims_32 = capacity_victims(&visible, &key);
        assert_eq!(
            victims_32, expected_28,
            "{mode}: 8->32 must evict exactly the newest 28"
        );

        let focus_val = focus_of(&visible);
        update_gaps(&mut visible, &key, 64, focus_val.as_ref()).expect("32->64 succeeds");
        let victims_seq = capacity_victims(&visible, &key);
        assert_eq!(
            victims_seq, expected_30,
            "{mode}: 8->32->64 must evict exactly the newest 30"
        );

        // Direct hidden-style jump on the matched tree.
        let focus_val = focus_of(&hidden);
        update_gaps(&mut hidden, &key, 64, focus_val.as_ref()).expect("8->64 succeeds");
        let victims_direct = capacity_victims(&hidden, &key);
        assert_eq!(
            victims_direct, victims_seq,
            "{mode}: direct 8->64 must evict exactly the staged victim set"
        );

        // Identical survivor trees (shares included via Node equality).
        assert_eq!(
            retained_tree(&visible, &key),
            retained_tree(&hidden, &key),
            "{mode}: survivor trees must match after both sequences"
        );
    }
}

#[test]
fn gap_repair_trigger_is_shape_dependent() {
    let (domain_64, _) = domain_for(64);
    let hints = helper_hints();
    let resolve = |_: &NodeId| hints;

    let layout = admit_roster(FocusRule::Newest);
    let (_, key) = domain_for(8);
    let deep = retained_tree(&layout, &key);
    assert!(
        project(&deep, domain_64.bounds, domain_64.gap).is_err(),
        "serial-admission tree must be gap-infeasible at 64/64"
    );

    let balanced = balanced_tree(ROSTER);
    assert!(project(&balanced, domain_64.bounds, domain_64.gap).is_ok());
    let hinted = project_with_hints(&balanced, domain_64.bounds, domain_64.gap, &resolve);
    assert!(
        hinted.is_ok(),
        "balanced tree must project hinted (flags allowed)"
    );
}

/// Balanced binary 40-leaf tree: alternating axes, equal shares, minimal depth.
fn balanced_tree(n: usize) -> Node {
    fn build(ids: &[usize], depth: u32, next: &mut usize) -> Node {
        if ids.len() == 1 {
            return Node::Leaf {
                id: NodeId(format!("b{:02}", ids[0])),
            };
        }
        let mid = ids.len() / 2;
        let axis = if depth.is_multiple_of(2) {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        let id = NodeId(format!("g{next}"));
        *next += 1;
        Node::Group {
            id,
            axis,
            children: vec![
                build(&ids[..mid], depth + 1, next),
                build(&ids[mid..], depth + 1, next),
            ],
            shares: vec![1, 1],
        }
    }
    build(&(0..n).collect::<Vec<_>>(), 0, &mut 0)
}

/// Stale-observation reproducer for the pre-fix select route: hidden period
/// commits nothing to the roster domain, then a raw `UpdateGaps` adoption
/// followed by a reconcile built from the same pre-adopt all-tiled rows.
/// This layout-level sequence re-tiles the silent victims (0 victims, 40
/// tiled) and is kept as numeric characterization of the old behavior.
mod hidden_reveal_route {
    use super::{
        EXPECTED_AT_64, FocusRule, ROSTER, admission_sequences, admit_roster, capacity_victims,
        domain_for, domain_for_output, newest_suffix, project, reconcile, retained_tree,
        tiled_links, token, update_gaps,
    };
    use crate::directional::WindowId;

    #[test]
    fn reveal_adopts_then_reconciles_without_hidden_commits() {
        let mut layout = admit_roster(FocusRule::Newest);
        let (_, key) = domain_for(8);
        let tree_before = retained_tree(&layout, &key);
        let geom_before = layout.domain_geometry(&key);
        let outer_before = layout.domain_outer_gap(&key);

        // Hidden period: a gap adoption on an unknown domain refuses and
        // commits nothing anywhere.
        let (other_domain, other_key) = domain_for_output("display-0", "ws-0", 64);
        let refused = layout.update_domain_gaps(
            &other_key,
            other_domain.bounds,
            other_domain.gap,
            64,
            &WindowId(String::new()),
            &[],
        );
        assert!(refused.is_err());
        assert_eq!(
            retained_tree(&layout, &key),
            tree_before,
            "hidden period must commit nothing to the roster domain"
        );
        assert_eq!(
            layout.domain_geometry(&key),
            geom_before,
            "roster retained gaps must stay 8/8 while hidden"
        );
        assert_eq!(layout.domain_outer_gap(&key), outer_before);

        // Reveal adoption, then a follow-on reconcile built from the same
        // pre-adopt all-tiled rows.
        let focused = token(ROSTER - 1);
        let deep_before = retained_tree(&layout, &key);
        let adopted = update_gaps(&mut layout, &key, 64, Some(&focused))
            .expect("reveal adoption must project");
        let survivor_geometry = adopted.geometry.len();
        let order = admission_sequences(&layout, &key);
        let expected_30 = newest_suffix(&order, EXPECTED_AT_64);
        assert_eq!(
            capacity_victims(&layout, &key),
            expected_30,
            "reveal adoption must evict exactly the newest 30"
        );
        assert_eq!(tiled_links(&layout, &key), ROSTER - EXPECTED_AT_64);
        assert_eq!(survivor_geometry, ROSTER - EXPECTED_AT_64);

        // The stale follow-on reconcile re-tiles the silent victims:
        // convergence flag-adopts the exceptions observed as tiled, so the
        // repair is undone with zero logged adoptions.
        let reconciled = reconcile(&mut layout, &key, 64, Some(&focused));
        let retiled_geometry = reconciled.geometry.len();
        assert_eq!(
            capacity_victims(&layout, &key),
            std::collections::BTreeSet::new(),
            "follow-on reconcile must leave zero capacity victims"
        );
        assert_eq!(tiled_links(&layout, &key), ROSTER);
        assert_eq!(retiled_geometry, ROSTER);
        let retiled_tree = retained_tree(&layout, &key);
        assert_ne!(
            retiled_tree, deep_before,
            "re-admission must rebuild a new survivor tree"
        );
        let (domain_64, _) = domain_for(64);
        assert!(
            project(&retiled_tree, domain_64.bounds, domain_64.gap).is_ok(),
            "re-admitted tree must project at 64/64"
        );
    }
}
