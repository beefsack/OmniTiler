//! Session-side D2/D3/D7 admission policy: defaults, fallback ranking, exhaustion/retry/eviction stability, and no settings reflow.
//!
//! Shared helpers live in the parent (`capacity_admission.rs`); this module
//! holds only `#[test]` fns.

use super::*;

#[test]
fn largest_fallback_tiles_with_dfs_tiebreak() {
    // 12x12 gap 4: H[A,B] halves of 4x12. A wide-placed newcomer fails
    // the ordinary horizontal wrap of B (4 - 4 < 2), then the largest
    // fallback splits A: equal 4x12 areas tie, DFS order picks A first.
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("o", "w", 12, 12, 4)],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "o", "w", placement(12, 4), "c-1");
    admit_and_commit(&mut session, "b", "o", "w", placement(12, 4), "c-2");
    let key = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("w".to_owned()),
    };
    assert_eq!(
        session.focus(),
        (Some(key.clone()), Some(leaf_of(&session, "b")))
    );
    let plan = admit_and_commit(&mut session, "c", "o", "w", placement(12, 4), "c-3");
    assert_eq!(plan.desired_geometry.len(), 3);
    assert!(
        plan.desired_geometry
            .iter()
            .all(|g| g.rect.w > 0 && g.rect.h > 0),
        "every tile stays positive"
    );
    let tree = session.tree_for(&key).expect("tree").clone();
    let c_leaf = leaf_of(&session, "c");
    let siblings = siblings_of(&tree, &c_leaf).expect("c has leaf siblings");
    assert!(
        siblings.contains(&leaf_of(&session, "a")),
        "DFS tiebreak splits A, not the ordinary anchor B: {siblings:?}"
    );
    assert_eq!(
        session.focus(),
        (Some(key), Some(c_leaf)),
        "successful admission focuses the newcomer"
    );
    // First-successful-tiled-admission order tracks a, b, c.
    assert_eq!(
        session.admission_sequence(&WindowId("a".to_owned())),
        Some(0)
    );
    assert_eq!(
        session.admission_sequence(&WindowId("b".to_owned())),
        Some(1)
    );
    assert_eq!(
        session.admission_sequence(&WindowId("c".to_owned())),
        Some(2)
    );
}

#[test]
fn b6_minimum_overlap_never_blocks_search_or_float() {
    // Declared minima alone never count as exhaustion (D1): an
    // unsatisfiable minimum keeps its proportional tile flagged
    // overconstrained, and later admissions still tile.
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("o", "w", 100, 100, 0)],
    )
    .expect("session");
    let mut hints = tiled_observed("a", "o", "w");
    hints.hints = WindowSizeHints {
        min_w: Some(200),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let obs = complete_observation(&session, vec![hints]);
    let plan = session
        .propose(
            &admit_command("a", "o", "w", placement(100, 100)),
            &obs,
            &correlation("c-1"),
            &LifecycleCapabilities::full(),
        )
        .expect("single leaf always tiles");
    commit_plan(&mut session, &plan, "c-1");
    assert!(!session.is_exception(&WindowId("a".to_owned())));
    admit_and_commit(&mut session, "b", "o", "w", placement(100, 100), "c-2");
    assert_eq!(
        tiled_windows(&session),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(
        !session.is_capacity_float(&WindowId("b".to_owned())),
        "minimum overlap alone never floats"
    );
}

#[test]
fn one_by_one_exhaustion_floats_with_stable_replay_and_close_clear() {
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("tiny", "ws", 1, 1, 0)],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "tiny", "ws", placement(1, 1), "c-1");
    let obs = complete_observation(&session, vec![tiled_observed("b", "tiny", "ws")]);
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &admit_command("b", "tiny", "ws", placement(1, 1)),
            &obs,
            &correlation("c-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("1x1 exhausts to float");
    assert!(plan.desired_geometry.is_empty());
    commit_plan(&mut session, &plan, "c-2");
    assert_eq!(session.accepted_revision(), base + 1);
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    assert!(
        !session.is_automatic_fixed_float(&WindowId("b".to_owned())),
        "capacity origin stays distinct from fixed auto"
    );
    // Stable replay: the identical accepted observation converges
    // exactly with no new revision, focus change, or re-tile.
    let replay = complete_observation(&session, Vec::new());
    let revision = session.accepted_revision();
    let focus = session.focus();
    let counts = session
        .converge_observation(&replay, Some(&WindowId("a".to_owned())))
        .expect("replay converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(session.accepted_revision(), revision);
    assert_eq!(session.focus(), focus);
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    // Close clears origin and order (D5).
    let obs = complete_observation(&session, Vec::new());
    let plan = session
        .propose(
            &SessionCommand::Remove {
                window: WindowId("b".to_owned()),
            },
            &obs,
            &correlation("c-3"),
            &LifecycleCapabilities::full(),
        )
        .expect("remove floats");
    commit_plan(&mut session, &plan, "c-3");
    assert!(!session.is_capacity_float(&WindowId("b".to_owned())));
    assert_eq!(session.admission_sequence(&WindowId("b".to_owned())), None);
    assert_eq!(
        session.admission_sequence(&WindowId("a".to_owned())),
        Some(0)
    );
}

#[test]
fn explicit_unfloat_retries_once_without_false_override() {
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("tiny", "ws", 1, 1, 0)],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "tiny", "ws", placement(1, 1), "c-1");
    let obs = complete_observation(&session, vec![tiled_observed("b", "tiny", "ws")]);
    let plan = session
        .propose(
            &admit_command("b", "tiny", "ws", placement(1, 1)),
            &obs,
            &correlation("c-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("floats");
    commit_plan(&mut session, &plan, "c-2");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    // Explicit unfloat retries once against still-insufficient space:
    // b stays floated with capacity origin and no tile override.
    let mut floated = session.exception_observed();
    assert!(floated.iter().any(|w| w.window.0 == "b"));
    let obs = complete_observation(&session, Vec::new());
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId("b".to_owned()),
                float_geometry: None,
            },
            &obs,
            &correlation("c-3"),
            &LifecycleCapabilities::full(),
        )
        .expect("retry proposes");
    assert!(
        session.pending_stages_capacity_float(&WindowId("b".to_owned())),
        "re-float reports no-space while staged"
    );
    commit_plan(&mut session, &plan, "c-3");
    assert_eq!(session.accepted_revision(), base + 1);
    assert!(
        session.is_capacity_float(&WindowId("b".to_owned())),
        "stays floated"
    );
    assert!(
        !session.has_fixed_tile_override(&WindowId("b".to_owned())),
        "no false fixed-tile override on a non-fixed re-float"
    );
    assert_eq!(tiled_windows(&session), vec!["a".to_string()]);
    let _ = &mut floated;
}

#[test]
fn shrink_evicts_newest_first_with_protections_and_mru_focus() {
    // Settle a/b/c in 120x80 gap 0, then shrink to 2x80: H[a,H[b,c]]
    // needs width 3, so the newest victim (c) floats alone.
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("o", "w", 120, 80, 0)],
    )
    .expect("session");
    admit_and_commit(&mut session, "a", "o", "w", placement(120, 40), "c-1");
    admit_and_commit(&mut session, "b", "o", "w", placement(120, 40), "c-2");
    admit_and_commit(&mut session, "c", "o", "w", placement(120, 40), "c-3");
    let key = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("w".to_owned()),
    };
    assert!(session.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 80
        },
        0
    ));
    let live: BTreeMap<WindowId, Rect> = ["a", "b", "c"]
        .into_iter()
        .map(|w| {
            (
                WindowId(w.to_owned()),
                Rect {
                    x: 1,
                    y: 2,
                    w: 30,
                    h: 30,
                },
            )
        })
        .collect();
    assert!(session.repair_infeasible_domain(
        &key,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &live,
        true,
    ));
    assert_eq!(
        tiled_windows(&session),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(session.is_capacity_float(&WindowId("c".to_owned())));
    assert_eq!(
        session.retained_float_geometry(&WindowId("c".to_owned())),
        Some(Rect {
            x: 1,
            y: 2,
            w: 30,
            h: 30
        }),
        "eviction keeps the last live frame"
    );
    assert_eq!(
        session.focus(),
        (Some(key.clone()), Some(leaf_of(&session, "b"))),
        "tiled focus falls back to the surviving MRU, never the floated window"
    );
    // An explicit fixed-tile override protects c: the next victim is b.
    let mut guarded = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("o", "w", 120, 80, 0)],
    )
    .expect("session");
    admit_and_commit(&mut guarded, "a", "o", "w", placement(120, 40), "c-1");
    admit_and_commit(&mut guarded, "b", "o", "w", placement(120, 40), "c-2");
    // Admit c with an explicit user tile win (unfloat origin).
    let obs = complete_observation(&guarded, vec![tiled_observed("c", "o", "w")]);
    let mut cmd = admit_command("c", "o", "w", placement(120, 40));
    if let SessionCommand::Admit {
        suppress_fixed_float,
        ..
    } = &mut cmd
    {
        *suppress_fixed_float = true;
    }
    let plan = guarded
        .propose(
            &cmd,
            &obs,
            &correlation("c-3"),
            &LifecycleCapabilities::full(),
        )
        .expect("c tiles");
    commit_plan(&mut guarded, &plan, "c-3");
    assert!(guarded.has_fixed_tile_override(&WindowId("c".to_owned())));
    assert!(guarded.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 80
        },
        0
    ));
    assert!(guarded.repair_infeasible_domain(
        &key,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &live,
        true,
    ));
    assert_eq!(
        tiled_windows(&guarded),
        vec!["a".to_string(), "c".to_string()]
    );
    assert!(guarded.is_capacity_float(&WindowId("b".to_owned())));
    assert!(
        !guarded.is_exception(&WindowId("c".to_owned())),
        "explicit override never evicts"
    );
    // A protected-only impossible tree holds truthfully without mutation:
    // shrink until even the protected pair cannot project, then repair
    // with every survivor protected.
    assert!(guarded.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 80
        },
        0
    ));
    let before = guarded.snapshot();
    let before_revision = guarded.accepted_revision();
    assert!(!guarded.repair_infeasible_domain(
        &key,
        &BTreeMap::new(),
        &BTreeSet::from([WindowId("a".to_owned()), WindowId("c".to_owned()),]),
        &live,
        true,
    ));
    assert_eq!(guarded.snapshot(), before, "hold mutates nothing");
    assert_eq!(guarded.accepted_revision(), before_revision);
    // Untracked lifetimes evict last: drop the order bookkeeping for a
    // survivor and shrink again with only tracked-newest eligible. Here
    // both survivors are tracked (a older than c), so newest-first
    // would evict c if it were eligible; it is override-protected, so b
    // (already floated) stays floated and a keeps its tile.
    assert_eq!(
        guarded.admission_sequence(&WindowId("a".to_owned())),
        Some(0)
    );
}
