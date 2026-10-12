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
//! - Invalidation: a sibling close invalidates the stored slot, so restore
//!   falls back to fresh ordinary admission.

use std::collections::BTreeMap;

use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
use tiler_core::contract::Observation;
use tiler_core::directional::{Node, NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, ObservedWindow, OutputDomain, Session, SessionObservation};
use tiler_core::size_hints::WindowSizeHints;

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

#[test]
fn mnz_invalidation_sibling_close_falls_back_to_fresh_admission() {
    let mut session = seed_mru();
    let pre_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    converge(&mut session, &["win-a", "win-c"], Some("win-c"), 41);
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_some()
    );

    // A sibling close restructures the tree, invalidating the stored slot.
    converge(&mut session, &["win-a"], Some("win-a"), 42);

    // Restoring B now falls back to fresh ordinary admission, not the old
    // slot, and the stale slot is gone.
    converge(&mut session, &["win-a", "win-b"], None, 43);
    let snapshot = session.snapshot();
    assert!(
        snapshot.windows.iter().any(|link| link.window.0 == "win-b"),
        "win-b admitted"
    );
    let fresh_tree = session
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    assert_ne!(
        fresh_tree, pre_tree,
        "invalid slot falls back to fresh admission"
    );
    assert!(
        session
            .minimized_slot(&WindowId("win-b".to_owned()))
            .is_none(),
        "invalid slot consumed"
    );
    assert_eq!(
        focused_window(&session).as_deref(),
        Some("win-a"),
        "fresh admission keeps retained focus"
    );
}
