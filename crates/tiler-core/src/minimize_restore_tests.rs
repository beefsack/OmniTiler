#![forbid(unsafe_code)]
//! D23 shared-core minimize/restore regressions (REQ-MNZ-01/02/03).
//!
//! Drives the real `Session::converge_observation` primitive, the path the
//! adapters use for complete observations: a native minimize arrives as an
//! omission, a native restore as a re-observed window.
//!
//! - MNZ-01: minimizing a middle tile releases its allocation, siblings
//!   reflow, focus falls to the MRU window, and the old slot is stored.
//! - MNZ-02: restore returns the window to its stored old slot and the core
//!   writes no focus (the host owns any restore focus).
//! - MNZ-03: minimizing a workspace's sole window retains the workspace with
//!   focus none (must not fail closed) and a stored restore slot.
//! - Best effort (2026-10-13 selection, option (b)): an intervening focus
//!   change, resize, new window, sibling close (with a survivor), swap,
//!   other-sibling minimize/restore roundtrip, or gaps/resolution change
//!   must not alone invalidate the slot: restore rejoins in the surviving
//!   old group at the old index, or regrouped with a live former neighbor
//!   on the stored axis and side with the stored old share, and never
//!   writes focus.
//! - Fresh ordinary admission only when no live former neighbor remains,
//!   the domain is gone, or the window was sent elsewhere. Restart discards
//!   slots (session-only). Reporting a minimized window's close forgets its
//!   slot via `Session::forget_minimized_slot`; a bare absence from
//!   observation stays a minimize.

use std::collections::BTreeMap;

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::contract::{
    AckOutcome, AdapterAck, Observation, PostObservation, ResizeCapabilities, ResizeMode,
    ResizePostObservation,
};
use crate::directional::{
    Axis, Capabilities, Direction, Node, NodeId, OutputId, WindowId, WorkspaceId,
};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, Session, SessionObservation};
use crate::size_hints::WindowSizeHints;

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}

fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}

fn domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
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

fn plain(window: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn observation(
    session: &Session,
    windows: Vec<ObservedWindow>,
    fingerprint: u64,
) -> SessionObservation {
    SessionObservation {
        observation: Observation::new(
            session.owner().clone(),
            session.generation().clone(),
            session.accepted_revision(),
            fingerprint,
        ),
        windows,
    }
}

fn converge(session: &mut Session, names: &[&str], focused: Option<&str>, fingerprint: u64) {
    let windows = names.iter().map(|name| plain(name)).collect();
    let focused = focused.map(|name| WindowId(name.to_owned()));
    session
        .converge_observation(
            &observation(session, windows, fingerprint),
            focused.as_ref(),
        )
        .expect("converge");
}

fn leaf_of(session: &Session, window: &str) -> NodeId {
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| link.window.0 == window)
        .unwrap_or_else(|| panic!("window {window} linked"))
        .leaf
        .clone()
}

fn focused_window(session: &Session) -> Option<String> {
    let (domain, leaf) = session.focus();
    let (domain, leaf) = (domain?, leaf?);
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| {
            link.leaf == leaf && link.output == domain.output && link.workspace == domain.workspace
        })
        .map(|link| link.window.0.clone())
}

/// Seed H-style A/B/C with MRU history A,C,B (B focused).
fn seed_mru() -> Session {
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    converge(&mut session, &["win-a"], Some("win-a"), 11);
    converge(&mut session, &["win-a", "win-b"], Some("win-b"), 12);
    converge(
        &mut session,
        &["win-a", "win-b", "win-c"],
        Some("win-c"),
        13,
    );
    let key = session.domains().first().expect("domain").key();
    assert!(
        session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())),
        "sync focus to win-b"
    );
    assert_eq!(focused_window(&session).as_deref(), Some("win-b"));
    session
}

fn engine_window(name: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(name.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
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
        hints: WindowSizeHints::none(),
    }
}

fn engine_reconcile(
    engine: &mut Engine,
    key: &DomainKey,
    windows: Vec<EngineWindow>,
    focused: &str,
    fingerprint: u64,
) -> CoreReply {
    engine.handle(&CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: CorrelationId::parse(&format!("corr-mnz-03-{fingerprint}")).expect("valid"),
        revision: 0,
        fingerprint,
        domain: domain(),
        domain_key: key.clone(),
        outer_gap: 0,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Reconcile,
    })
}

/// Seed a sole window, hand it to a real engine, and drive the
/// sole-minimize reconcile leg: the session is retained (not retired) with
/// an emptied tiling layer, focus none, and a stored restore slot. Returns
/// the engine plus the pre-minimize tree/leaf for the restore legs below.
fn sole_minimized_engine() -> (Engine, DomainKey, Node, NodeId) {
    // Seed the sole window through the converge primitive, then hand the
    // session to a real engine so the reconcile path (converge, empty
    // retire check, projection) runs exactly as the adapters drive it.
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    converge(&mut session, &["win-a"], Some("win-a"), 31);
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    let pre_leaf = leaf_of(&session, "win-a");
    let key = domain().key();
    let mut engine = Engine::new();
    engine.sync_binding(session.owner(), session.generation());
    engine.store_committed(key.clone(), session, 0);

    // Sole minimize arrives as explicit empty evidence with focus none.
    // The retire check must retain the slot (not retire it), reporting an
    // empty projection at the converged revision.
    let pre_revision = engine.session(&key).expect("seeded").accepted_revision();
    match engine_reconcile(&mut engine, &key, vec![], "", 32) {
        CoreReply::Projection(plan) => {
            assert!(plan.geometry.is_empty());
            assert_eq!((plan.focus_domain, plan.focus_leaf), (None, None));
            assert_eq!(plan.base_revision, pre_revision + 1);
        }
        other => panic!("sole minimize must project empty, got {other:?}"),
    }
    assert!(
        engine.contains(&key),
        "sole-minimized session retained, not retired"
    );
    let kept = engine.session(&key).expect("retained");
    assert!(
        kept.tree_for(&domain().key()).is_none(),
        "tiling layer emptied"
    );
    assert_eq!(kept.focus(), (None, None), "focus none");
    assert!(
        kept.minimized_slot(&WindowId("win-a".to_owned())).is_some(),
        "restore slot survives the retire check"
    );
    (engine, key, pre_tree, pre_leaf)
}

#[test]
fn mnz_01_minimize_middle_releases_allocation_with_mru_focus_and_stored_slot() {
    let mut session = seed_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    let pre_b_leaf = leaf_of(&session, "win-b");

    // Native minimize B arrives as an omission; C is the MRU survivor.
    converge(&mut session, &["win-a", "win-c"], Some("win-c"), 21);

    let snapshot = session.snapshot();
    let leaves: Vec<String> = snapshot
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .collect();
    assert_eq!(leaves, vec!["win-a".to_owned(), "win-c".to_owned()]);
    assert!(
        session.tree_for(&domain().key()).is_some(),
        "domain retains survivors"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "focus falls to MRU survivor"
    );
    let slot = session
        .minimized_slot(&WindowId("win-b".to_owned()))
        .expect("old slot stored");
    assert_eq!(slot.leaf, pre_b_leaf);
    assert_eq!(slot.tree, pre_tree);
}

#[test]
fn mnz_02_restore_returns_to_old_slot_with_no_focus_write() {
    let mut session = seed_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    let pre_b_leaf = leaf_of(&session, "win-b");
    converge(&mut session, &["win-a", "win-c"], Some("win-c"), 21);

    // Native restore re-observes B; the host natively focuses B, but the
    // core must not adopt that focus write.
    converge(
        &mut session,
        &["win-a", "win-b", "win-c"],
        Some("win-b"),
        22,
    );

    let restored_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    assert_eq!(
        restored_tree, pre_tree,
        "restore returns to the stored old slot"
    );
    assert_eq!(leaf_of(&session, "win-b"), pre_b_leaf);
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "slot consumed by restore"
    );
}

#[test]
fn mnz_03_sole_minimize_retains_workspace_with_focus_none_and_stored_slot() {
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    converge(&mut session, &["win-a"], Some("win-a"), 31);
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    let pre_leaf = leaf_of(&session, "win-a");

    // Sole-window minimize must not fail closed: the workspace is retained
    // empty with focus none and a stored restore slot.
    converge(&mut session, &[], None, 32);

    assert_eq!(session.domains().len(), 1, "workspace retained");
    assert!(
        session.tree_for(&domain().key()).is_none(),
        "tiling layer emptied"
    );
    assert_eq!(session.focus(), (None, None), "focus none");
    let slot = session
        .minimized_slot(&WindowId("win-a".to_owned()))
        .expect("restore slot stored");
    assert_eq!(slot.tree, pre_tree);
    assert_eq!(slot.leaf, pre_leaf);

    // Restore returns the sole window to its old slot, still with no core
    // focus write (the host owns restore focus).
    converge(&mut session, &["win-a"], Some("win-a"), 33);
    let restored = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    assert_eq!(restored, pre_tree);
    assert_eq!(session.focus(), (None, None));
}

#[test]
fn mnz_03_sole_minimize_retained_through_engine_reconcile_and_restores_old_slot() {
    let (mut engine, key, pre_tree, pre_leaf) = sole_minimized_engine();

    // A later native restore re-observes the sole window (host-focused, as
    // on the realistic taskbar path). The stored-slot converge must win
    // over the all-new-IDs fresh-admission route: the tree and leaf come
    // back exactly, the core still writes no focus, and the slot is
    // consumed.
    let reply = engine_reconcile(&mut engine, &key, vec![engine_window("win-a")], "win-a", 33);
    let restored = engine.session(&key).expect("retained after restore");
    let restored_tree = restored
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    assert_eq!(
        restored_tree, pre_tree,
        "engine-path restore returns to the stored old slot, not fresh admission"
    );
    assert_eq!(leaf_of(restored, "win-a"), pre_leaf);
    assert_eq!(restored.focus(), (None, None), "core writes no focus");
    assert!(
        restored
            .minimized_slot(&WindowId("win-a".to_owned()))
            .is_none(),
        "slot consumed by restore"
    );
    // The no-focus-write restore converges to focus none with the old-slot
    // tree back; the reconcile operation projects that converged state
    // instead of reporting a focus mismatch.
    match reply {
        CoreReply::Projection(plan) => {
            assert_eq!(plan.geometry.len(), 1);
            assert_eq!(
                plan.geometry[0].window,
                WindowId("win-a".to_owned()),
                "restored tile projected"
            );
            assert_eq!(
                (plan.focus_domain, plan.focus_leaf),
                (None, None),
                "no focus fabricated"
            );
            assert_eq!(plan.base_revision, restored.accepted_revision());
        }
        other => panic!("sole restore must project old-slot geometry, got {other:?}"),
    }
}

#[test]
fn mnz_sole_restore_with_unknown_focus_still_reports_mismatch() {
    let (mut engine, key, pre_tree, _) = sole_minimized_engine();

    // The same restore observation, but the observed focus names a window
    // that is not a converged tile: the converged old-slot state still
    // persists, yet the operation reports the genuine focus mismatch.
    let reply = engine_reconcile(
        &mut engine,
        &key,
        vec![engine_window("win-a")],
        "win-ghost",
        33,
    );
    match reply {
        CoreReply::Rejected { kind, .. } => {
            assert_eq!(kind, "focus-mismatch");
        }
        other => panic!("unknown-focus restore must report focus-mismatch, got {other:?}"),
    }
    let restored = engine.session(&key).expect("retained after restore");
    let restored_tree = restored
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    assert_eq!(
        restored_tree, pre_tree,
        "converged old-slot state persists past the refusal"
    );
    assert_eq!(restored.focus(), (None, None));
}

/// Window names in deterministic tree leaf order for one-domain sessions.
fn leaf_window_order(session: &Session) -> Vec<String> {
    let snapshot = session.snapshot();
    let tree = snapshot
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    let mut leaves = Vec::new();
    collect_test_leaves(&tree, &mut leaves);
    leaves
        .into_iter()
        .map(|leaf| {
            snapshot
                .windows
                .iter()
                .find(|link| link.leaf == leaf)
                .unwrap_or_else(|| panic!("leaf {leaf:?} linked"))
                .window
                .0
                .clone()
        })
        .collect()
}

fn collect_test_leaves(node: &Node, out: &mut Vec<NodeId>) {
    match node {
        Node::Leaf { id } => out.push(id.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect_test_leaves(child, out);
            }
        }
    }
}

/// Root group shares for one-domain sessions (`None` for a root leaf).
fn root_shares(session: &Session) -> Option<Vec<u64>> {
    match session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree")
    {
        Node::Group { shares, .. } => Some(shares),
        Node::Leaf { .. } => None,
    }
}

/// Membership-matching observation over the current tiled set (plus
/// exceptions) for propose/acknowledge/verify cycles.
fn complete_obs(session: &Session, fingerprint: u64) -> SessionObservation {
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| ObservedWindow {
            window: link.window.clone(),
            output: link.output.clone(),
            workspace: link.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        })
        .collect();
    windows.extend(session.exception_observed());
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    SessionObservation {
        observation: Observation::new(
            session.owner().clone(),
            session.generation().clone(),
            session.accepted_revision(),
            fingerprint,
        ),
        windows,
    }
}

fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).expect("valid correlation")
}

/// Committed keyboard resize of the focused tiled window (survivor shares
/// change; topology and membership do not).
fn resize_committed(session: &mut Session, window: &str, direction: Direction, corr: &str) {
    let key = domain().key();
    assert!(
        session.sync_focus_from_window(&key, &WindowId(window.to_owned())),
        "sync focus to resized {window}"
    );
    let obs = complete_obs(session, 500 + session.accepted_revision());
    let base = session.accepted_revision();
    let plan = session
        .propose_resize(
            &key,
            &WindowId(window.to_owned()),
            direction,
            ResizeMode::Outwards,
            0,
            &obs,
            &correlation(corr),
            &ResizeCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("resize {window:?}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_resize(&ResizePostObservation::new(
            Observation::new(owner(), generation(), base, 960 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify resize");
}

/// Committed directional move of a tiled window (order/shape change with
/// surviving membership).
fn move_committed(session: &mut Session, window: &str, direction: Direction, corr: &str) {
    let key = domain().key();
    assert!(
        session.sync_focus_from_window(&key, &WindowId(window.to_owned())),
        "sync focus to mover {window}"
    );
    let obs = complete_obs(session, 700 + session.accepted_revision());
    let base = session.accepted_revision();
    let plan = session
        .propose_move(
            &key,
            &WindowId(window.to_owned()),
            direction,
            &obs,
            &correlation(corr),
            &Capabilities::full(),
        )
        .unwrap_or_else(|e| panic!("move {window:?}: {e:?}"))
        .expect_moved("tiled move");
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_move(&PostObservation::new(
            Observation::new(owner(), generation(), base, 970 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify move");
}

/// Committed pointer resize of a tiled window to an absolute work-area
/// boundary (exact pixel shares; focus retained, topology unchanged).
fn pointer_resize_committed(
    session: &mut Session,
    window: &str,
    direction: Direction,
    boundary: i32,
    corr: &str,
) {
    let key = domain().key();
    let obs = complete_obs(session, 500 + session.accepted_revision());
    let base = session.accepted_revision();
    let plan = session
        .propose_pointer_resize(
            &key,
            &WindowId(window.to_owned()),
            direction,
            boundary,
            &obs,
            &correlation(corr),
            &ResizeCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("pointer resize {window:?}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_resize(&ResizePostObservation::new(
            Observation::new(owner(), generation(), base, 960 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify pointer resize");
}

/// Immediate parent group axis/shares of a leaf in the one-domain tree.
fn parent_group_of(session: &Session, leaf: &str) -> (Axis, Vec<u64>) {
    let tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    find_parent_test(&tree, &NodeId(leaf.to_owned())).expect("parent group")
}

fn find_parent_test(node: &Node, leaf: &NodeId) -> Option<(Axis, Vec<u64>)> {
    match node {
        Node::Leaf { .. } => None,
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            if children.iter().any(|child| match child {
                Node::Leaf { id } => id == leaf,
                Node::Group { .. } => false,
            }) {
                return Some((*axis, shares.clone()));
            }
            children
                .iter()
                .find_map(|child| find_parent_test(child, leaf))
        }
    }
}

/// Seed H-style A/B/C with unequal shares and MRU history A,C,B (B focused):
/// the matrix Given for every restore leg.
fn seed_unequal_mru() -> Session {
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    converge(&mut session, &["win-a"], Some("win-a"), 11);
    converge(&mut session, &["win-a", "win-b"], Some("win-b"), 12);
    converge(
        &mut session,
        &["win-a", "win-b", "win-c"],
        Some("win-c"),
        13,
    );
    resize_committed(&mut session, "win-c", Direction::Left, "corr-seed-resize");
    let key = domain().key();
    assert!(
        session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())),
        "sync focus to win-b"
    );
    assert!(
        root_shares(&session).is_some_and(|shares| shares != vec![1, 1]),
        "Given carries unequal shares"
    );
    assert_eq!(focused_window(&session).as_deref(), Some("win-b"));
    session
}

/// Minimize B from the unequal seed; the MRU survivor C keeps focus.
fn minimize_b(session: &mut Session) {
    converge(session, &["win-a", "win-c"], Some("win-c"), 21);
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_some(),
        "B slot stored"
    );
    assert_eq!(
        focused_window(session).as_deref(),
        Some("win-c"),
        "MRU survivor keeps focus"
    );
}

/// Native restore: re-observe B (host-focused) and expect no core focus
/// write plus a consumed slot.
fn restore_b(session: &mut Session, fingerprint: u64) {
    restore_b_with(session, &["win-a", "win-b", "win-c"], fingerprint);
}

/// Native restore over an explicit observed set (for legs with extra
/// survivors such as an admitted newcomer).
fn restore_b_with(session: &mut Session, names: &[&str], fingerprint: u64) {
    let focused = Some(WindowId("win-b".to_owned()));
    let windows = names.iter().map(|name| plain(name)).collect();
    session
        .converge_observation(
            &observation(session, windows, fingerprint),
            focused.as_ref(),
        )
        .expect("converge");
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "slot consumed by restore"
    );
}

#[test]
fn mnz_best_effort_focus_change_rejoins_with_no_focus_write() {
    let mut session = seed_unequal_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    minimize_b(&mut session);

    // Focus A instead of C: focus alone changes no topology.
    let key = domain().key();
    assert!(session.sync_focus_from_window(&key, &WindowId("win-a".to_owned())));
    restore_b(&mut session, 22);

    assert_eq!(
        session
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree"),
        pre_tree,
        "focus alone rejoins the old neighborhood exactly"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-a"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_resize_rejoins_beside_former_neighbor() {
    let mut session = seed_unequal_mru();
    minimize_b(&mut session);

    // Intervening resize changes survivor shares but removes no neighbor.
    resize_committed(&mut session, "win-c", Direction::Left, "corr-leg-resize");
    let current_shares = root_shares(&session).expect("root group");
    assert_eq!(current_shares.len(), 2, "survivors share one group");
    restore_b(&mut session, 23);

    assert_eq!(
        leaf_window_order(&session),
        vec!["win-a".to_owned(), "win-b".to_owned(), "win-c".to_owned()],
        "B rejoins between its former neighbors, not fresh after C"
    );
    assert_eq!(
        root_shares(&session).expect("root group"),
        current_shares,
        "regroup keeps the survivor slot shares"
    );
    assert_eq!(
        parent_group_of(&session, "leaf-win-b"),
        (Axis::Vertical, vec![1, 1]),
        "B regroups with former sibling C carrying the stored old shares"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_new_window_rejoins_former_neighborhood() {
    let mut session = seed_unequal_mru();
    minimize_b(&mut session);

    // A new window wraps the focused survivor; former neighbors survive.
    converge(
        &mut session,
        &["win-a", "win-c", "win-d"],
        Some("win-d"),
        24,
    );
    restore_b_with(&mut session, &["win-a", "win-b", "win-c", "win-d"], 25);

    assert_eq!(
        leaf_window_order(&session),
        vec![
            "win-a".to_owned(),
            "win-b".to_owned(),
            "win-c".to_owned(),
            "win-d".to_owned()
        ],
        "B rejoins beside former neighbor C, not fresh beside newcomer D"
    );
    assert_eq!(
        parent_group_of(&session, "leaf-win-b"),
        (Axis::Vertical, vec![1, 1]),
        "B regroups with former sibling C carrying the stored old shares"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-d"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_sibling_close_rejoins_by_survivor() {
    let mut session = seed_unequal_mru();
    minimize_b(&mut session);

    // Close A, retaining C: B must rejoin by its surviving former neighbor
    // on its old side (before C), not fresh after C.
    converge(&mut session, &["win-c"], Some("win-c"), 26);
    assert!(
        session
            .minimized_slot(&WindowId("win-a".to_owned()))
            .is_some(),
        "closed sibling carries its own slot"
    );
    converge(&mut session, &["win-b", "win-c"], Some("win-b"), 27);
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "slot consumed by restore"
    );

    assert_eq!(
        leaf_window_order(&session),
        vec!["win-b".to_owned(), "win-c".to_owned()],
        "B rejoins before surviving neighbor C (old side)"
    );
    assert_eq!(
        parent_group_of(&session, "leaf-win-b"),
        (Axis::Vertical, vec![1, 1]),
        "sole-survivor wrap keeps the stored axis with the stored old shares"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_swap_rejoins_neighborhood() {
    let mut session = seed_unequal_mru();
    minimize_b(&mut session);

    // Swap the survivors: the order change alone must not invalidate.
    move_committed(&mut session, "win-c", Direction::Left, "corr-leg-swap");
    assert_eq!(
        leaf_window_order(&session),
        vec!["win-c".to_owned(), "win-a".to_owned()],
        "swap leg precondition"
    );
    restore_b(&mut session, 28);

    assert_eq!(
        leaf_window_order(&session),
        vec!["win-b".to_owned(), "win-c".to_owned(), "win-a".to_owned()],
        "B rejoins beside former neighbor C on its old side"
    );
    assert_eq!(
        parent_group_of(&session, "leaf-win-b"),
        (Axis::Vertical, vec![1, 1]),
        "B regroups with former sibling C carrying the stored old shares"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

/// Seed H-style A/B/C with a skewed old B share (pointer-exact V split,
/// B outweighs C) and MRU history A,C,B (B focused).
fn seed_skewed_mru() -> Session {
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    converge(&mut session, &["win-a"], Some("win-a"), 81);
    converge(&mut session, &["win-a", "win-b"], Some("win-b"), 82);
    converge(
        &mut session,
        &["win-a", "win-b", "win-c"],
        Some("win-c"),
        83,
    );
    pointer_resize_committed(
        &mut session,
        "win-b",
        Direction::Down,
        360,
        "corr-skew-resize",
    );
    let key = domain().key();
    assert!(
        session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())),
        "sync focus to win-b"
    );
    assert_eq!(focused_window(&session).as_deref(), Some("win-b"));
    session
}

/// Stored old parent axis/shares of win-b from its minimize slot.
fn stored_old_group(session: &Session) -> (Axis, Vec<u64>) {
    let slot = session
        .minimized_slot(&WindowId("win-b".to_owned()))
        .expect("B slot stored");
    find_parent_test(&slot.tree, &slot.leaf).expect("stored parent group")
}

#[test]
fn mnz_best_effort_skewed_share_regroups_stored_fraction() {
    let mut session = seed_skewed_mru();
    minimize_b(&mut session);
    let (old_axis, old_shares) = stored_old_group(&session);
    assert_eq!(old_axis, Axis::Vertical);
    assert!(
        old_shares[0] > old_shares[1],
        "skewed Given carries B outweighing C: {old_shares:?}"
    );

    // Intervening root resize breaks the exact fast path only.
    resize_committed(
        &mut session,
        "win-c",
        Direction::Left,
        "corr-skew-leg-resize",
    );
    let current_shares = root_shares(&session).expect("root group");
    restore_b(&mut session, 84);

    assert_eq!(
        leaf_window_order(&session),
        vec!["win-a".to_owned(), "win-b".to_owned(), "win-c".to_owned()],
        "B rejoins between its former neighbors"
    );
    assert_eq!(
        root_shares(&session).expect("root group"),
        current_shares,
        "regroup keeps the resized survivor slot shares"
    );
    assert_eq!(
        parent_group_of(&session, "leaf-win-b"),
        (Axis::Vertical, old_shares),
        "regroup carries the stored old shares verbatim, never an average"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_skewed_wrap_preserves_old_ratio() {
    let mut session = seed_skewed_mru();
    minimize_b(&mut session);
    let (old_axis, old_shares) = stored_old_group(&session);
    assert_eq!(old_axis, Axis::Vertical);
    assert!(
        old_shares[0] > old_shares[1],
        "skewed Given carries B outweighing C: {old_shares:?}"
    );

    // Close A: the sole survivor is the root leaf, so restore wraps it on
    // the stored axis with the stored ratio, not equal halves.
    converge(&mut session, &["win-c"], Some("win-c"), 85);
    converge(&mut session, &["win-b", "win-c"], Some("win-b"), 86);
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "slot consumed by restore"
    );

    assert_eq!(
        leaf_window_order(&session),
        vec!["win-b".to_owned(), "win-c".to_owned()],
        "B rejoins before surviving neighbor C (old side)"
    );
    assert_eq!(
        parent_group_of(&session, "leaf-win-b"),
        (Axis::Vertical, old_shares),
        "wrap preserves the stored old ratio, never equal halves"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_surviving_group_inserts_at_old_index() {
    let mut session = seed_skewed_mru();
    // Grow the old parent to three children: the R1 wrap plus the R2b
    // same-axis append keep the original group id.
    move_committed(&mut session, "win-a", Direction::Down, "corr-survivor-m1");
    move_committed(&mut session, "win-a", Direction::Up, "corr-survivor-m2");
    assert_eq!(
        leaf_window_order(&session),
        vec!["win-b".to_owned(), "win-c".to_owned(), "win-a".to_owned()],
        "three-child old group precondition"
    );
    // A newcomer nests inside the old group around C; the group id survives.
    let key = domain().key();
    assert!(session.sync_focus_from_window(&key, &WindowId("win-c".to_owned())));
    converge(
        &mut session,
        &["win-a", "win-b", "win-c", "win-d"],
        Some("win-d"),
        87,
    );
    assert!(session.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    converge(
        &mut session,
        &["win-a", "win-c", "win-d"],
        Some("win-c"),
        88,
    );
    let (stored_axis, stored_shares) = stored_old_group(&session);
    assert_eq!(stored_axis, Axis::Vertical);
    assert_eq!(
        stored_shares.len(),
        3,
        "stored old group carries three children: {stored_shares:?}"
    );
    // An intervening root resize breaks the exact fast path while the old
    // group id, axis, and survivors stay live (keyboard resize preserves
    // the pair total, so the comparable entrant equals the stored share).
    resize_committed(&mut session, "win-a", Direction::Up, "corr-survivor-resize");
    let current_shares = root_shares(&session).expect("root group");
    assert_ne!(
        current_shares,
        stored_shares[1..].to_vec(),
        "exact fast path is broken by the resize"
    );

    converge(
        &mut session,
        &["win-a", "win-b", "win-c", "win-d"],
        Some("win-b"),
        89,
    );
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "slot consumed by restore"
    );

    assert_eq!(
        leaf_window_order(&session),
        vec![
            "win-b".to_owned(),
            "win-c".to_owned(),
            "win-d".to_owned(),
            "win-a".to_owned()
        ],
        "B rejoins at its old index in the surviving old group"
    );
    // The comparable entrant equals the stored old share here (the keyboard
    // resize preserves the pair total); a proportional average would admit
    // 269 instead of 359.
    let mut expected = vec![stored_shares[0]];
    expected.extend_from_slice(&current_shares);
    assert_eq!(
        root_shares(&session).expect("root group"),
        expected,
        "surviving-group insert keeps the stored old share with live survivors"
    );
    match session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree")
    {
        Node::Group { children, .. } => assert!(
            matches!(&children[0], Node::Leaf { id } if id.0 == "leaf-win-b"),
            "B is a direct child at the old index, not nested"
        ),
        Node::Leaf { .. } => panic!("root group expected"),
    }
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-a"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_other_minimize_restore_roundtrip_rejoins() {
    let mut session = seed_unequal_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    minimize_b(&mut session);

    // An interleaved sibling minimize/restore roundtrip leaves B's
    // neighborhood intact.
    converge(&mut session, &["win-a"], Some("win-a"), 31);
    converge(&mut session, &["win-a", "win-c"], Some("win-c"), 32);
    assert!(
        session
            .minimized_slot(&WindowId("win-c".to_owned()))
            .is_none(),
        "sibling slot consumed by its own restore"
    );
    restore_b(&mut session, 33);

    assert_eq!(
        session
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree"),
        pre_tree,
        "interleaved sibling roundtrip rejoins the old neighborhood"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-a"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_gaps_change_rejoins_with_current_projection() {
    let mut session = seed_unequal_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    minimize_b(&mut session);

    // Gaps/resolution changes reproject; topology is untouched.
    assert!(
        session.update_domain_gaps(
            &domain().key(),
            Rect {
                x: 0,
                y: 0,
                w: 1024,
                h: 768
            },
            8,
        ),
        "gap update applies"
    );
    restore_b(&mut session, 34);

    assert_eq!(
        session
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree"),
        pre_tree,
        "gaps change rejoins the old neighborhood"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_session_restart_discards_slots_into_fresh_admission() {
    let mut session = seed_unequal_mru();
    minimize_b(&mut session);
    assert_eq!(session.minimized_count(), 1);

    // Restart: a new owner generation starts with no restore context.
    let mut restarted = Session::new(
        owner(),
        GenerationId::parse("gen-2").expect("valid"),
        0,
        7,
        vec![domain()],
    )
    .expect("session");
    assert_eq!(restarted.minimized_count(), 0);
    converge(&mut restarted, &["win-a", "win-c"], Some("win-c"), 41);
    converge(
        &mut restarted,
        &["win-a", "win-b", "win-c"],
        Some("win-b"),
        42,
    );

    assert!(
        restarted
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window.0 == "win-b"),
        "win-b admitted fresh after restart"
    );
    assert_eq!(restarted.minimized_count(), 0, "no slot survives a restart");
    let _ = session;
}

#[test]
fn mnz_close_while_minimized_forgets_slot_into_fresh_admission() {
    let mut session = seed_unequal_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    minimize_b(&mut session);

    // Explicit close (not minimize): the stored slot is forgotten.
    assert!(
        session.forget_minimized_slot(&WindowId("win-b".to_owned())),
        "close forgets the minimized slot"
    );
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none()
    );
    assert!(
        !session.forget_minimized_slot(&WindowId("win-b".to_owned())),
        "repeat forget is false"
    );

    // A later same-id observation is a brand-new window: fresh ordinary
    // admission beside the focused survivor, never the old slot.
    converge(
        &mut session,
        &["win-a", "win-b", "win-c"],
        Some("win-b"),
        43,
    );
    assert_ne!(
        session
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree"),
        pre_tree,
        "forgotten slot falls back to fresh admission"
    );
    assert_eq!(
        leaf_window_order(&session),
        vec!["win-a".to_owned(), "win-c".to_owned(), "win-b".to_owned()],
        "fresh admission wraps the focused survivor"
    );
}

#[test]
fn mnz_close_sole_minimized_allows_session_retire() {
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    converge(&mut session, &["win-a"], Some("win-a"), 51);
    converge(&mut session, &[], None, 52);
    assert!(
        session.has_live_minimized_slot(),
        "minimized slot retains the empty session"
    );

    assert!(
        session.forget_minimized_slot(&WindowId("win-a".to_owned())),
        "close forgets the sole slot"
    );
    assert!(
        !session.has_live_minimized_slot(),
        "no live slot remains, the empty session may retire"
    );
}

#[test]
fn mnz_fresh_fallback_absent_neighbors() {
    let mut session = seed_unequal_mru();
    minimize_b(&mut session);

    // Close every former neighbor: no survivor remains, so restore falls
    // back to fresh ordinary admission on the emptied domain.
    converge(&mut session, &["win-c"], Some("win-c"), 61);
    converge(&mut session, &[], None, 62);
    converge(&mut session, &["win-b"], Some("win-b"), 63);

    let snapshot = session.snapshot();
    assert!(
        snapshot.windows.iter().any(|link| link.window.0 == "win-b"),
        "win-b admitted"
    );
    assert_eq!(
        leaf_window_order(&session),
        vec!["win-b".to_owned()],
        "sole survivor on the emptied domain"
    );
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "stale slot consumed"
    );
}

#[test]
fn mnz_fresh_fallback_domain_gone() {
    let (mut engine, key, _, _) = sole_minimized_engine();

    // The domain is gone: release drops the retained minimized session, so
    // the next observation seeds a fresh domain with no restore context.
    engine.remove(&key);
    assert!(!engine.contains(&key), "domain released");
    let reply = engine_reconcile(&mut engine, &key, vec![engine_window("win-a")], "win-a", 44);
    match reply {
        CoreReply::Tiled(plan) => {
            assert_eq!(plan.geometry.len(), 1, "fresh admit projects one tile")
        }
        other => panic!("released domain must seed fresh, got {other:?}"),
    }
    let fresh = engine.session(&key).expect("fresh session");
    assert!(
        fresh
            .minimized_slot(&WindowId("win-a".to_owned()))
            .is_none(),
        "no slot survives domain release"
    );
}

fn homed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
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
        hints: WindowSizeHints::none(),
    }
}

fn converge_two(
    session: &mut Session,
    windows: Vec<ObservedWindow>,
    focused: Option<&str>,
    fingerprint: u64,
) {
    let focused = focused.map(|name| WindowId(name.to_owned()));
    session
        .converge_observation(
            &SessionObservation {
                observation: Observation::new(
                    session.owner().clone(),
                    session.generation().clone(),
                    session.accepted_revision(),
                    fingerprint,
                ),
                windows,
            },
            focused.as_ref(),
        )
        .expect("converge");
}

#[test]
fn mnz_fresh_fallback_sent_elsewhere() {
    let ws2 = OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let mut session =
        Session::new(owner(), generation(), 0, 7, vec![domain(), ws2]).expect("session");
    converge_two(
        &mut session,
        vec![
            homed("win-a", "out-1", "ws-1"),
            homed("win-b", "out-1", "ws-1"),
            homed("win-c", "out-1", "ws-1"),
            homed("win-d", "out-1", "ws-2"),
        ],
        Some("win-b"),
        71,
    );
    // Minimize B on ws-1; ws-2 stays complete.
    converge_two(
        &mut session,
        vec![
            homed("win-a", "out-1", "ws-1"),
            homed("win-c", "out-1", "ws-1"),
            homed("win-d", "out-1", "ws-2"),
        ],
        Some("win-c"),
        72,
    );
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_some(),
        "B slot stored on ws-1"
    );

    // B reappears on ws-2 (sent elsewhere): the ws-1 slot is consumed and
    // B takes fresh ordinary admission on ws-2.
    converge_two(
        &mut session,
        vec![
            homed("win-a", "out-1", "ws-1"),
            homed("win-c", "out-1", "ws-1"),
            homed("win-b", "out-1", "ws-2"),
            homed("win-d", "out-1", "ws-2"),
        ],
        Some("win-b"),
        73,
    );
    let link = session
        .snapshot()
        .windows
        .into_iter()
        .find(|link| link.window.0 == "win-b")
        .expect("win-b linked");
    assert_eq!(link.workspace, WorkspaceId("ws-2".to_owned()));
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "stale slot consumed"
    );
    let ws1_tree = session
        .snapshot()
        .domains
        .into_iter()
        .find(|d| d.workspace.0 == "ws-1")
        .expect("ws-1")
        .tree
        .expect("tree");
    let mut ws1_leaves = Vec::new();
    collect_test_leaves(&ws1_tree, &mut ws1_leaves);
    assert_eq!(ws1_leaves.len(), 2, "ws-1 keeps its survivors");
}

/// Engine reconcile carrying explicitly closed ids (D23 close distinction).
fn engine_reconcile_closed(
    engine: &mut Engine,
    key: &DomainKey,
    windows: Vec<EngineWindow>,
    closed: &[&str],
    focused: &str,
    fingerprint: u64,
) -> CoreReply {
    engine.handle(&CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: CorrelationId::parse(&format!("corr-mnz-closed-{fingerprint}"))
            .expect("valid"),
        revision: 0,
        fingerprint,
        domain: domain(),
        domain_key: key.clone(),
        outer_gap: 0,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: closed
            .iter()
            .map(|name| WindowId((*name).to_owned()))
            .collect(),
        command: CoreCommand::Reconcile,
    })
}

/// Seed H-style A/B/C through a real engine (reconcile legs, B focused).
fn triple_engine() -> (Engine, DomainKey) {
    let key = domain().key();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    for (names, focused, fingerprint) in [
        (vec!["win-a"], "win-a", 101),
        (vec!["win-a", "win-b"], "win-b", 102),
        (vec!["win-a", "win-b", "win-c"], "win-b", 103),
    ] {
        let windows = names.iter().map(|name| engine_window(name)).collect();
        match engine_reconcile(&mut engine, &key, windows, focused, fingerprint) {
            CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
            other => panic!("seed leg must plan, got {other:?}"),
        }
    }
    assert!(engine.contains(&key), "triple retained");
    (engine, key)
}

fn engine_windows(engine: &Engine, key: &DomainKey) -> Vec<String> {
    let mut names: Vec<String> = engine
        .session(key)
        .expect("session")
        .snapshot()
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn mnz_close_minimized_via_engine_forgets_slot_without_readmit() {
    let (mut engine, key) = triple_engine();

    // Minimize B as a bare omission: the old slot is stored.
    match engine_reconcile(
        &mut engine,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        "win-c",
        104,
    ) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("minimize leg must plan, got {other:?}"),
    }
    assert!(
        engine
            .session(&key)
            .expect("session")
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_some(),
        "minimize stores the old slot"
    );

    // The minimized window closes: same omission plus the explicit close
    // report forgets the slot without readmitting the window.
    match engine_reconcile_closed(
        &mut engine,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        &["win-b"],
        "win-c",
        105,
    ) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("close leg must plan, got {other:?}"),
    }
    let session = engine.session(&key).expect("survivors retain the session");
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "explicit close forgets the minimized slot"
    );
    assert_eq!(
        engine_windows(&engine, &key),
        vec!["win-a".to_owned(), "win-c".to_owned()],
        "closed window is not readmitted"
    );
}

#[test]
fn mnz_ordinary_close_mints_no_slot() {
    let (mut engine, key) = triple_engine();

    // A visible tiled window closes in the same turn it disappears: the
    // omission plus the close report must never mint a slot for it.
    match engine_reconcile_closed(
        &mut engine,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        &["win-b"],
        "win-c",
        106,
    ) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("ordinary close must plan, got {other:?}"),
    }
    assert!(
        engine
            .session(&key)
            .expect("session")
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "ordinary close mints no slot"
    );
    assert_eq!(
        engine_windows(&engine, &key),
        vec!["win-a".to_owned(), "win-c".to_owned()],
        "survivors keep membership"
    );

    // A later same-id window is a brand-new admission, never an old-slot
    // restore.
    match engine_reconcile(
        &mut engine,
        &key,
        vec![
            engine_window("win-a"),
            engine_window("win-b"),
            engine_window("win-c"),
        ],
        "win-b",
        107,
    ) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("re-admission must plan, got {other:?}"),
    }
    assert!(
        engine
            .session(&key)
            .expect("session")
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "no slot lingers after fresh re-admission"
    );
}

#[test]
fn mnz_close_sole_minimized_retires_session_via_engine() {
    let (mut engine, key, _, _) = sole_minimized_engine();
    assert!(engine.contains(&key), "sole-minimized session retained");

    // The sole minimized window closes: the slot is forgotten and the
    // emptied session retires on the same turn instead of lingering.
    match engine_reconcile_closed(&mut engine, &key, vec![], &["win-a"], "", 108) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("sole close must plan, got {other:?}"),
    }
    assert!(
        !engine.contains(&key),
        "slot-only session retires once its sole slot is forgotten"
    );
}

#[test]
fn mnz_close_cleanup_fenced_on_diverged_observation() {
    let (mut engine, key) = triple_engine();
    match engine_reconcile(
        &mut engine,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        "win-c",
        109,
    ) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("minimize leg must plan, got {other:?}"),
    }
    assert!(
        engine
            .session(&key)
            .expect("session")
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_some(),
        "minimize stores the old slot"
    );

    // A close report on a diverged (foreign-owner) observation must not
    // forget: the adapter has to retry with the close retained.
    let diverged = engine.handle(&CoreEvent {
        owner: OwnerId::parse("owner-2").expect("valid"),
        generation: generation(),
        correlation: CorrelationId::parse("corr-mnz-diverged").expect("valid"),
        revision: 0,
        fingerprint: 110,
        domain: domain(),
        domain_key: key.clone(),
        outer_gap: 0,
        focused_window: WindowId("win-c".to_owned()),
        windows: vec![engine_window("win-a"), engine_window("win-c")],
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![WindowId("win-b".to_owned())],
        command: CoreCommand::Reconcile,
    });
    assert!(
        matches!(diverged, CoreReply::Diverged(_)),
        "foreign owner diverges, got {diverged:?}"
    );
    assert!(
        engine
            .session(&key)
            .expect("session")
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_some(),
        "diverged close report retains the slot for retry"
    );
}
