//! Gap-repair discrepancy replay: visible staged shrink (8->32->64) against a
//! hidden-style direct jump (8->64) on matched retained preimages, plus a
//! reveal-route mirror (adopt gaps, then reconcile) and a shape-dependence
//! witness. Geometry below is generic representative stress input (numeric
//! regression values), not host detail.
//!
//! Relocated from the Windows adapter suite when the per-domain adapter
//! route was removed: the replay covers portable core gap-repair behavior,
//! so it lives with the core per-domain tests and builds its observations
//! through [`crate::per_domain_fixture`].

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreReply};
use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::directional::{OutputId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::{Rect, project};
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::per_domain_fixture::{
    fingerprint, reconcile_event_for_floating as build_reconcile_event_for_floating,
};
use crate::session::{DomainKey, OutputDomain};
use crate::size_hints::{WindowSizeHints, project_with_hints};

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
fn carried_rows() -> Vec<(WindowId, Rect, WindowSizeHints, bool)> {
    (0..ROSTER)
        .map(|i| (token(i), birth_rect(), helper_hints(), false))
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

fn owner() -> OwnerId {
    OwnerId::parse("tiler-windows").expect("owner")
}

fn generation() -> GenerationId {
    GenerationId::parse("aa").expect("generation")
}

fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).expect("correlation")
}

fn revision_of(engine: &Engine, key: &DomainKey) -> u64 {
    engine
        .session(key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0)
}

fn dispatch(
    engine: &mut Engine,
    key: &DomainKey,
    domain: &OutputDomain,
    outer_gap: i32,
    command: CoreCommand,
    focused: Option<&WindowId>,
    correlation_id: &str,
) -> CoreReply {
    let carried = carried_rows();
    let fp = fingerprint(
        &carried
            .iter()
            .map(|(id, rect, _, _)| (id.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let mut event = build_reconcile_event_for_floating(
        &owner(),
        &generation(),
        &correlation(correlation_id),
        revision_of(engine, key),
        fp,
        domain,
        key,
        outer_gap,
        &carried,
        focused,
    );
    event.command = command;
    engine.handle(&event)
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

fn admit_roster(focus: FocusRule) -> Engine {
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let (domain, key) = domain_for(8);
    for i in 0..ROSTER {
        let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = (0..=i)
            .map(|j| (token(j), birth_rect(), helper_hints(), false))
            .collect();
        let fp = fingerprint(
            &carried
                .iter()
                .map(|(id, rect, _, _)| (id.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let focused = focus.admission_focus(i);
        let mut event = build_reconcile_event_for_floating(
            &owner(),
            &generation(),
            &correlation(&format!("admit-{i:02}")),
            revision_of(&engine, &key),
            fp,
            &domain,
            &key,
            8,
            &carried,
            focused.as_ref(),
        );
        event.command = CoreCommand::Reconcile;
        let reply = engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "admission leg {i} must converge, got {reply:?}"
        );
    }
    let session = engine.session(&key).expect("retained session");
    assert_eq!(session.snapshot().windows.len(), ROSTER);
    assert_eq!(session.capacity_float_count(), 0);
    engine
}

fn capacity_victims(engine: &Engine, key: &DomainKey) -> BTreeSet<String> {
    let Some(session) = engine.session(key) else {
        return BTreeSet::new();
    };
    (0..ROSTER)
        .map(token)
        .filter(|id| session.is_capacity_float(id))
        .map(|id| id.0.clone())
        .collect()
}

fn admission_sequences(engine: &Engine, key: &DomainKey) -> BTreeMap<String, u64> {
    let Some(session) = engine.session(key) else {
        return BTreeMap::new();
    };
    (0..ROSTER)
        .map(token)
        .map(|id| {
            let seq = session.admission_sequence(&id).expect("tracked order");
            (id.0.clone(), seq)
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

fn tiled_links(engine: &Engine, key: &DomainKey) -> usize {
    engine
        .session(key)
        .map(|session| session.snapshot().windows.len())
        .unwrap_or(0)
}

fn retained_tree(engine: &Engine, key: &DomainKey) -> Node {
    engine
        .session(key)
        .expect("session")
        .snapshot()
        .domains
        .into_iter()
        .find(|view| view.output == key.output && view.workspace == key.workspace)
        .expect("domain view")
        .tree
        .expect("retained tree")
}

fn update_gaps(
    engine: &mut Engine,
    key: &DomainKey,
    gap: i32,
    focused: Option<&WindowId>,
    correlation_id: &str,
) -> CoreReply {
    let (domain, _) = domain_for(gap);
    dispatch(
        engine,
        key,
        &domain,
        gap,
        CoreCommand::UpdateGaps,
        focused,
        correlation_id,
    )
}

fn reconcile(
    engine: &mut Engine,
    key: &DomainKey,
    gap: i32,
    focused: Option<&WindowId>,
    correlation_id: &str,
) -> CoreReply {
    let (domain, _) = domain_for(gap);
    dispatch(
        engine,
        key,
        &domain,
        gap,
        CoreCommand::Reconcile,
        focused,
        correlation_id,
    )
}

#[test]
fn matched_visible_vs_hidden_gap_repair() {
    assert!(
        carried_rows()
            .iter()
            .all(|(_, _, hints, floating)| *hints == helper_hints() && !floating),
        "replay inputs must carry identical hints and tiled flags"
    );
    for focus in [FocusRule::Newest, FocusRule::None, FocusRule::First] {
        let mode = focus.label();
        let mut visible = admit_roster(focus);
        let mut hidden = admit_roster(focus);
        let (_, key) = domain_for(8);
        // Matched session inputs: identical snapshots (trees with shares,
        // links), identical admission order.
        assert_eq!(
            visible.session(&key).expect("session").snapshot(),
            hidden.session(&key).expect("session").snapshot(),
            "{mode}: retained sessions must match before replay"
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

        let focus_of = |engine: &Engine| -> Option<WindowId> {
            match focus {
                FocusRule::None => None,
                FocusRule::Newest => Some(token(ROSTER - 1)),
                FocusRule::First => Some(token(0)),
            }
            .filter(|id| {
                engine.session(&key).is_some_and(|session| {
                    session
                        .snapshot()
                        .windows
                        .iter()
                        .any(|link| &link.window == id)
                })
            })
        };

        // Staged visible sequence: 8 -> 32 -> 64.
        let focus = focus_of(&visible);
        let reply_32 = update_gaps(
            &mut visible,
            &key,
            32,
            focus.as_ref(),
            &format!("{mode}-gap32"),
        );
        assert!(matches!(reply_32, CoreReply::Projection(_)));
        let victims_32 = capacity_victims(&visible, &key);
        assert_eq!(
            victims_32, expected_28,
            "{mode}: 8->32 must evict exactly the newest 28"
        );

        let focus = focus_of(&visible);
        let reply_64 = update_gaps(
            &mut visible,
            &key,
            64,
            focus.as_ref(),
            &format!("{mode}-gap64"),
        );
        assert!(matches!(reply_64, CoreReply::Projection(_)));
        let victims_seq = capacity_victims(&visible, &key);
        assert_eq!(
            victims_seq, expected_30,
            "{mode}: 8->32->64 must evict exactly the newest 30"
        );

        // Direct hidden-style jump on the matched tree.
        let focus = focus_of(&hidden);
        let reply_direct = update_gaps(
            &mut hidden,
            &key,
            64,
            focus.as_ref(),
            &format!("{mode}-gap64-direct"),
        );
        assert!(matches!(reply_direct, CoreReply::Projection(_)));
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

    let engine = admit_roster(FocusRule::Newest);
    let (_, key) = domain_for(8);
    let deep = retained_tree(&engine, &key);
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
/// This Engine-level sequence re-tiles the silent victims (0 victims, 40
/// tiled) and is kept as numeric characterization of the old behavior.
/// The corrected production route (`selection.rs`) refreshes observation
/// flags from current session exceptions between the two dispatches; that
/// fix is pinned by `tiling_sys::select_repair_route_tests`, not here.
/// `adopt_gaps_for_route` itself is `pub(super)` in `tiling_sys::cross_drag`,
/// unreachable from integration tests without production edits (out of
/// scope), so this mirrors its protocol sequence with identical builders.
mod hidden_reveal_route {
    use super::{
        EXPECTED_AT_64, FocusRule, ROSTER, admission_sequences, admit_roster,
        build_reconcile_event_for_floating, capacity_victims, correlation, domain_for,
        domain_for_output, fingerprint, generation, newest_suffix, owner, project, reconcile,
        retained_tree, tiled_links, token, update_gaps,
    };
    use crate::boundary::{CoreCommand, CoreReply};

    #[test]
    fn reveal_adopts_then_reconciles_without_hidden_commits() {
        let mut engine = admit_roster(FocusRule::Newest);
        let (_, key) = domain_for(8);
        let before = engine.session(&key).expect("session").snapshot();
        let gaps_before = engine
            .session(&key)
            .expect("session")
            .domains()
            .iter()
            .find(|domain| domain.key() == key)
            .expect("domain")
            .gap;

        // Hidden period: the adapter runs no tick legs for hidden domains, so
        // nothing dispatches here. A gap adoption on an unknown domain (the
        // active empty workspace) refuses and commits nothing anywhere.
        let (other_domain, other_key) = domain_for_output("display-0", "ws-0", 64);
        let mut event = build_reconcile_event_for_floating(
            &owner(),
            &generation(),
            &correlation("hidden-period-probe"),
            0,
            fingerprint(&[]),
            &other_domain,
            &other_key,
            64,
            &[],
            None,
        );
        event.command = CoreCommand::UpdateGaps;
        let refused = engine.handle(&event);
        assert!(matches!(refused, CoreReply::Rejected { .. }));
        assert_eq!(
            engine.session(&key).expect("session").snapshot(),
            before,
            "hidden period must commit nothing to the roster domain"
        );
        assert_eq!(
            engine
                .session(&key)
                .expect("session")
                .domains()
                .iter()
                .find(|domain| domain.key() == key)
                .expect("domain")
                .gap,
            gaps_before,
            "roster retained gaps must stay 8/8 while hidden"
        );

        // Reveal adoption, then a follow-on reconcile built from the same
        // pre-adopt all-tiled rows (the stale sequence the old select
        // route ran: it assembled rows once, adopted, then reconciled
        // without refreshing flags).
        let focused = token(ROSTER - 1);
        let deep_before = retained_tree(&engine, &key);
        let adopted = update_gaps(&mut engine, &key, 64, Some(&focused), "reveal-adopt");
        let survivor_geometry = match adopted {
            CoreReply::Projection(plan) => plan.geometry.len(),
            other => panic!("reveal adoption must project, got {other:?}"),
        };
        let order = admission_sequences(&engine, &key);
        let expected_30 = newest_suffix(&order, EXPECTED_AT_64);
        assert_eq!(
            capacity_victims(&engine, &key),
            expected_30,
            "reveal adoption must evict exactly the newest 30"
        );
        assert_eq!(tiled_links(&engine, &key), ROSTER - EXPECTED_AT_64);
        assert_eq!(survivor_geometry, ROSTER - EXPECTED_AT_64);

        // The stale follow-on reconcile re-tiles the silent victims:
        // convergence flag-adopts the exceptions observed as tiled, so the
        // repair is undone with zero logged adoptions (adopt_gaps_for_route
        // never registers floats; only a reconcile reply is adopted).
        let reconciled = reconcile(&mut engine, &key, 64, Some(&focused), "reveal-reconcile");
        let retiled_geometry = match reconciled {
            CoreReply::Projection(plan) => plan.geometry.len(),
            other => panic!("reveal reconcile must project, got {other:?}"),
        };
        assert_eq!(
            capacity_victims(&engine, &key),
            std::collections::BTreeSet::new(),
            "follow-on reconcile must leave zero capacity victims"
        );
        assert_eq!(tiled_links(&engine, &key), ROSTER);
        assert_eq!(retiled_geometry, ROSTER);
        let retiled_tree = retained_tree(&engine, &key);
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
