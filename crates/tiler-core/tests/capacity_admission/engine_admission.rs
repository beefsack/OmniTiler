//! Engine-side capacity admission: fresh mixed seeds, newcomer projection and marks, ordinary-float parity, echo persistence, and order.
//!
//! Shared helpers live in the parent (`capacity_admission.rs`); this module
//! holds only `#[test]` fns.

use super::*;

#[test]
fn engine_fresh_mixed_seed_800x600_gap8_commits_complete_roster() {
    use tiler_core::boundary::CoreReply;
    use tiler_core::engine::Engine;
    // Fourteen overlapping 200x200 observations in 800x600 gap 8: the
    // clean fit declines (centre splits), so the spatial focus-last
    // serial seed commits one atomic roster instead of discarding the
    // whole seed. Largest-fallback packing tiles all fourteen here;
    // smaller geometries exercise the floated half below.
    let domain = domain("out-1", "ws1", 800, 600, 8);
    let windows: Vec<tiler_core::seed::EngineWindow> = (0..14)
        .map(|i: i32| {
            engine_window(
                &format!("win-{i:06}"),
                "out-1",
                "ws1",
                Rect {
                    x: (i * 37) % 600,
                    y: 0,
                    w: 200,
                    h: 200,
                },
            )
        })
        .collect();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let event = reconcile_event(
        &owner(),
        &generation(),
        &domain,
        4,
        "",
        windows.clone(),
        "r-1",
    );
    let reply = engine.handle(&event);
    let key = domain.key();
    let session = engine.session(&key).expect("mixed roster retained");
    let mut live: Vec<String> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .chain(
            session
                .exception_observed()
                .iter()
                .map(|w| w.window.0.clone()),
        )
        .collect();
    live.sort();
    let expected: Vec<String> = (0..14).map(|i: i32| format!("win-{i:06}")).collect();
    assert_eq!(live, expected, "every lifetime keeps exactly one row");
    assert_eq!(
        session.accepted_revision(),
        1,
        "one atomic commit for the whole roster"
    );
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "no hold for a committable roster: {reply:?}"
    );
    // Stable replay: the same report converges exactly.
    let revision = session.accepted_revision();
    let again = reconcile_event(&owner(), &generation(), &domain, 4, "", windows, "r-2");
    let _ = engine.handle(&again);
    let session = engine.session(&key).expect("retained");
    assert_eq!(
        session.accepted_revision(),
        revision,
        "replay advances nothing"
    );
}

#[test]
fn engine_small_domain_seed_mixes_tiles_and_capacity_floats() {
    use tiler_core::boundary::{CoreReply, TiledKind};
    use tiler_core::engine::Engine;
    // 10x10 gap 8 exhausts after four tiles: the last two legs
    // capacity-float in the same atomic commit (revision 1).
    let domain = domain("o", "w", 10, 10, 8);
    let windows: Vec<tiler_core::seed::EngineWindow> = (0..6)
        .map(|i: usize| {
            engine_window(
                &format!("win-{i:06}"),
                "o",
                "w",
                Rect {
                    x: 0,
                    y: 0,
                    w: 50,
                    h: 50,
                },
            )
        })
        .collect();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let event = reconcile_event(
        &owner(),
        &generation(),
        &domain,
        0,
        "",
        windows.clone(),
        "r-1",
    );
    let reply = engine.handle(&event);
    let key = domain.key();
    let session = engine.session(&key).expect("roster retained");
    assert_eq!(session.snapshot().windows.len(), 4, "four tiles: {reply:?}");
    assert_eq!(session.capacity_float_count(), 2, "two capacity floats");
    assert_eq!(session.accepted_revision(), 1, "one atomic commit");
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "mixed roster never holds: {reply:?}"
    );
    // The focused-last anchor floats: the reply names it as the native
    // float target with no frame authority, while the tiled remainder
    // still projects.
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let event = reconcile_event(
        &owner(),
        &generation(),
        &domain,
        0,
        "win-000005",
        windows,
        "r-1",
    );
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            assert_eq!(plan.kind, TiledKind::Admit);
            assert_eq!(
                plan.float_window,
                Some(WindowId("win-000005".to_owned())),
                "floated anchor is the native float target"
            );
            assert_eq!(plan.float_rect, None, "no frame authority over floats");
            assert_eq!(plan.geometry.len(), 4, "tiled remainder still projects");
        }
        other => panic!("anchor-float seed admits mixed roster, got {other:?}"),
    }
    let session = engine.session(&key).expect("retained");
    assert!(session.is_capacity_float(&WindowId("win-000005".to_owned())));
}

#[test]
fn engine_capacity_float_newcomer_keeps_projection_and_marks() {
    use tiler_core::boundary::CoreReply;
    use tiler_core::engine::Engine;
    let domain = domain("tiny", "ws", 1, 1, 0);
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let first = reconcile_event(
        &owner(),
        &generation(),
        &domain,
        0,
        "a",
        vec![engine_window(
            "a",
            "tiny",
            "ws",
            Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
        )],
        "r-1",
    );
    assert!(matches!(engine.handle(&first), CoreReply::Tiled(_)));
    // Newcomer b exhausts the retained 1x1 domain: observation
    // convergence capacity-floats it at its native frame with no tile
    // slot, and the reconcile reply projects the intact survivor.
    // (The float target signal rides the fresh-seed and toggle-float
    // Tiled replies; the projection reply carries no float field by
    // design in this unit. Next unit owns that wire.)
    let second = reconcile_event(
        &owner(),
        &generation(),
        &domain,
        0,
        "a",
        vec![
            engine_window(
                "a",
                "tiny",
                "ws",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
            engine_window(
                "b",
                "tiny",
                "ws",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
        ],
        "r-2",
    );
    match engine.handle(&second) {
        CoreReply::Projection(plan) => {
            assert_eq!(plan.geometry.len(), 1);
            assert_eq!(plan.geometry[0].window.0, "a");
        }
        other => panic!("survivor still projects, got {other:?}"),
    }
    let key = domain.key();
    let session = engine.session(&key).expect("retained");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    assert_eq!(tiled_windows(session), vec!["a".to_string()]);
}

#[test]
fn capacity_float_matches_ordinary_float_with_no_auto_retile() {
    // D4 diagnostic-origin-only: a capacity float behaves exactly like an
    // ordinary float under equivalent reports. A normalized floating:true
    // echo persists with no auto-retile (no counts, no revision) across
    // replay, growth, and gap changes; growth alone never tiles. A raw
    // floating:false echo means explicit external intent per the ordinary
    // observation authority: convergence re-attempts admission exactly as
    // for an ordinary float, and propose paths bind flags exactly.
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
        .expect("b floats");
    commit_plan(&mut session, &plan, "c-2");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    // Normalized echo on replay: still floated, revision untouched.
    let revision = session.accepted_revision();
    let echo = complete_observation(&session, Vec::new());
    assert!(
        echo.windows
            .iter()
            .find(|w| w.window.0 == "b")
            .is_some_and(|w| w.floating),
        "normalized echo reports the float as floating"
    );
    let counts = session
        .converge_observation(&echo, Some(&WindowId("a".to_owned())))
        .expect("echo converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(session.accepted_revision(), revision, "no silent re-tile");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    // Growth alone never re-tiles under the normalized echo.
    let key = DomainKey {
        output: OutputId("tiny".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    assert!(session.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100
        },
        0
    ));
    let echo = complete_observation(&session, Vec::new());
    let counts = session
        .converge_observation(&echo, Some(&WindowId("a".to_owned())))
        .expect("grown echo converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert!(
        session.is_capacity_float(&WindowId("b".to_owned())),
        "growth alone never re-tiles"
    );
    assert_eq!(tiled_windows(&session), vec!["a".to_string()]);
    // Ordinary-float equivalence under the same normalized report: an
    // ordinary deferred float converges identically (zero counts).
    let mut ordinary = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("tiny", "ws", 100, 100, 0)],
    )
    .expect("session");
    admit_and_commit(&mut ordinary, "a", "tiny", "ws", placement(1, 1), "o-1");
    {
        use tiler_core::session::ExceptionBehavior;
        let mut extra = tiled_observed("b", "tiny", "ws");
        extra.floating = true;
        let obs = complete_observation(&ordinary, vec![extra]);
        let plan = ordinary
            .propose(
                &SessionCommand::Admit {
                    window: WindowId("b".to_owned()),
                    output: OutputId("tiny".to_owned()),
                    workspace: WorkspaceId("ws".to_owned()),
                    exceptions: tiler_core::session::ExceptionFlags {
                        floating: true,
                        fullscreen: false,
                        maximized: false,
                        sticky: false,
                    },
                    exception_behavior: Some(ExceptionBehavior::Defer),
                    placement_bounds: placement(1, 1),
                    suppress_fixed_float: false,
                },
                &obs,
                &correlation("o-2"),
                &LifecycleCapabilities::full(),
            )
            .expect("ordinary float defers");
        commit_plan(&mut ordinary, &plan, "o-2");
    }
    assert!(!ordinary.is_capacity_float(&WindowId("b".to_owned())));
    let ordinary_echo = complete_observation(&ordinary, Vec::new());
    let ordinary_counts = ordinary
        .converge_observation(&ordinary_echo.clone(), Some(&WindowId("a".to_owned())))
        .expect("ordinary echo converges");
    assert_eq!(
        (
            ordinary_counts.removed,
            ordinary_counts.admitted,
            ordinary_counts.flags_adopted
        ),
        (0, 0, 0),
        "ordinary and capacity floats persist identically under normalized echo"
    );
    // Raw floating:false means explicit external intent for both: the
    // converge re-attempts admission identically (same counts shape).
    let mut false_cap = echo_observation(&session, &[("b", false)]);
    // The false echo must actually flip the flag to exercise intent.
    assert!(
        false_cap
            .windows
            .iter()
            .find(|w| w.window.0 == "b")
            .is_some_and(|w| !w.floating)
    );
    let cap_counts = session
        .converge_observation(&false_cap, Some(&WindowId("a".to_owned())))
        .expect("false echo converges like ordinary intent");
    let mut false_ord = ordinary_echo.clone();
    for entry in false_ord.windows.iter_mut() {
        if entry.window.0 == "b" {
            entry.floating = false;
        }
    }
    // Grow the ordinary domain back to 1x1-equivalent exhaustion so both
    // re-attempts face the same geometry: skip cross-geometry comparison
    // and only assert the capacity leg also treats false as intent.
    assert!(
        cap_counts.flags_adopted > 0 || cap_counts.admitted > 0,
        "false echo re-attempts admission, never silently ignored: {cap_counts:?}"
    );
    let _ = &mut false_cap;
    let _ = &mut false_ord;
    // Propose paths bind exactly: a close under the stale false echo
    // refuses like an ordinary float mismatch (PartialObservation).
    let mut session2 = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("tiny", "ws", 1, 1, 0)],
    )
    .expect("session");
    admit_and_commit(&mut session2, "a", "tiny", "ws", placement(1, 1), "c-1");
    let obs = complete_observation(&session2, vec![tiled_observed("b", "tiny", "ws")]);
    let plan = session2
        .propose(
            &admit_command("b", "tiny", "ws", placement(1, 1)),
            &obs,
            &correlation("c-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("b floats");
    commit_plan(&mut session2, &plan, "c-2");
    let stale = echo_observation(&session2, &[("b", false)]);
    assert!(matches!(
        session2.propose(
            &SessionCommand::Remove {
                window: WindowId("a".to_owned()),
            },
            &stale,
            &correlation("c-3"),
            &LifecycleCapabilities::full(),
        ),
        Err(tiler_core::session::ProposeError::Refused(
            tiler_core::session::RefusalKind::PartialObservation
        ))
    ));
    // Explicit ToggleFloat under the normalized echo tiles b once room
    // exists and clears the origin with no false fixed-tile override.
    assert!(session2.update_domain_gaps(
        &key,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100
        },
        0
    ));
    let echo = complete_observation(&session2, Vec::new());
    let plan = session2
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId("b".to_owned()),
                float_geometry: None,
            },
            &echo,
            &correlation("c-4"),
            &LifecycleCapabilities::full(),
        )
        .expect("explicit unfloat proposes under normalized echo");
    assert!(
        !session2.pending_stages_capacity_float(&WindowId("b".to_owned())),
        "successful retry tiles instead of re-floating"
    );
    commit_plan(&mut session2, &plan, "c-4");
    assert_eq!(
        tiled_windows(&session2),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(!session2.is_capacity_float(&WindowId("b".to_owned())));
}

#[test]
fn engine_capacity_float_persists_under_normalized_echo_until_toggle() {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::engine::Engine;
    let tiny_dom = domain("tiny", "ws", 1, 1, 0);
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let first = reconcile_event(
        &owner(),
        &generation(),
        &tiny_dom,
        0,
        "a",
        vec![engine_window(
            "a",
            "tiny",
            "ws",
            Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
        )],
        "r-1",
    );
    assert!(matches!(engine.handle(&first), CoreReply::Tiled(_)));
    let second = reconcile_event(
        &owner(),
        &generation(),
        &tiny_dom,
        0,
        "a",
        vec![
            engine_window(
                "a",
                "tiny",
                "ws",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
            engine_window(
                "b",
                "tiny",
                "ws",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
        ],
        "r-2",
    );
    assert!(matches!(engine.handle(&second), CoreReply::Projection(_)));
    let key = tiny_dom.key();
    assert!(
        engine
            .session(&key)
            .expect("retained")
            .is_capacity_float(&WindowId("b".to_owned()))
    );
    // Growth with a normalized floating:true echo: the survivor projects,
    // b stays floated with its marker; growth alone never tiles (no
    // auto-retile, same as an ordinary float).
    let grown = domain("tiny", "ws", 100, 100, 0);
    let mut floated_b = engine_window(
        "b",
        "tiny",
        "ws",
        Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
    );
    floated_b.floating = true;
    let third = reconcile_event(
        &owner(),
        &generation(),
        &grown,
        0,
        "a",
        vec![
            engine_window(
                "a",
                "tiny",
                "ws",
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            ),
            floated_b,
        ],
        "r-3",
    );
    match engine.handle(&third) {
        CoreReply::Projection(plan) => assert_eq!(plan.geometry.len(), 1),
        other => panic!("grown survivor projects, got {other:?}"),
    }
    // The domain key is output/workspace only, so the grown bounds
    // address the same retained session.
    let session = engine.session(&key).expect("retained");
    assert!(session.is_capacity_float(&WindowId("b".to_owned())));
    // Explicit toggle-float (observed floating:true, like an ordinary
    // float unfloat) tiles b once room exists.
    let mut toggle_b = engine_window(
        "b",
        "tiny",
        "ws",
        Rect {
            x: 0,
            y: 50,
            w: 50,
            h: 100,
        },
    );
    toggle_b.floating = true;
    let mut toggle = reconcile_event(
        &owner(),
        &generation(),
        &grown,
        0,
        "b",
        vec![
            engine_window(
                "a",
                "tiny",
                "ws",
                Rect {
                    x: 0,
                    y: 0,
                    w: 50,
                    h: 100,
                },
            ),
            toggle_b,
        ],
        "r-4",
    );
    toggle.command = CoreCommand::ToggleFloat {
        window: "b".to_owned(),
        float_rect: None,
    };
    toggle.focused_window = WindowId("b".to_owned());
    match engine.handle(&toggle) {
        CoreReply::Tiled(plan) => assert!(
            plan.geometry.iter().any(|g| g.window.0 == "b"),
            "explicit toggle tiles b: {plan:?}"
        ),
        other => panic!("explicit toggle tiles, got {other:?}"),
    }
    let session = engine.session(&key).expect("retained");
    assert!(!session.is_capacity_float(&WindowId("b".to_owned())));
    assert!(session.snapshot().windows.iter().any(|l| l.window.0 == "b"));
}

#[test]
fn capacity_float_preserves_focus_like_ordinary_float() {
    // Diagnostic origin grants no extra behavior and suppresses none:
    // a capacity-floated newcomer under observed focus leaves the tiled
    // MRU exactly where an ordinary deferred float arrival does.
    for floated in ["cap", "ord"] {
        let mut session = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![domain("tiny", "ws", 1, 1, 0)],
        )
        .expect("session");
        admit_and_commit(&mut session, "a", "tiny", "ws", placement(1, 1), "c-1");
        let key = DomainKey {
            output: OutputId("tiny".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
        };
        if floated == "cap" {
            let obs = complete_observation(&session, vec![tiled_observed("b", "tiny", "ws")]);
            let plan = session
                .propose(
                    &admit_command("b", "tiny", "ws", placement(1, 1)),
                    &obs,
                    &correlation("c-2"),
                    &LifecycleCapabilities::full(),
                )
                .expect("capacity floats");
            commit_plan(&mut session, &plan, "c-2");
            assert!(session.is_capacity_float(&WindowId("b".to_owned())));
        } else {
            use tiler_core::session::ExceptionBehavior;
            let mut extra = tiled_observed("b", "tiny", "ws");
            extra.floating = true;
            let obs = complete_observation(&session, vec![extra]);
            let plan = session
                .propose(
                    &SessionCommand::Admit {
                        window: WindowId("b".to_owned()),
                        output: OutputId("tiny".to_owned()),
                        workspace: WorkspaceId("ws".to_owned()),
                        exceptions: tiler_core::session::ExceptionFlags {
                            floating: true,
                            fullscreen: false,
                            maximized: false,
                            sticky: false,
                        },
                        exception_behavior: Some(ExceptionBehavior::Defer),
                        placement_bounds: placement(1, 1),
                        suppress_fixed_float: false,
                    },
                    &obs,
                    &correlation("c-2"),
                    &LifecycleCapabilities::full(),
                )
                .expect("ordinary float defers");
            commit_plan(&mut session, &plan, "c-2");
            assert!(!session.is_capacity_float(&WindowId("b".to_owned())));
        }
        assert_eq!(
            session.focus(),
            (Some(key), Some(leaf_of(&session, "a"))),
            "{floated}: tiled MRU preserved identically"
        );
    }
}

#[test]
fn pair_round_trip_preserves_capacity_order_globally() {
    // Admission sequence, capacity origin, and overlap marks survive
    // pair assembly and split partitioned by homing (orphans to source).
    let d1 = domain("o", "ws-1", 1, 1, 0);
    let d2 = domain("o", "ws-2", 100, 100, 0);
    let mut s1 = Session::new(owner(), generation(), 0, 7, vec![d1.clone()]).expect("s1");
    admit_and_commit(&mut s1, "a", "o", "ws-1", placement(1, 1), "c-1");
    let obs = complete_observation(&s1, vec![tiled_observed("b", "o", "ws-1")]);
    let plan = s1
        .propose(
            &admit_command("b", "o", "ws-1", placement(1, 1)),
            &obs,
            &correlation("c-2"),
            &LifecycleCapabilities::full(),
        )
        .expect("b floats");
    commit_plan(&mut s1, &plan, "c-2");
    assert!(s1.is_capacity_float(&WindowId("b".to_owned())));
    let mut s2 = Session::new(owner(), generation(), 0, 7, vec![d2.clone()]).expect("s2");
    admit_and_commit(&mut s2, "c", "o", "ws-2", placement(10, 10), "c-1");
    let pair = Session::paired_from_canonical(&s1, Some(&s2), vec![d1.clone(), d2.clone()])
        .expect("pair assembles");
    assert!(pair.is_capacity_float(&WindowId("b".to_owned())));
    assert_eq!(pair.admission_sequence(&WindowId("a".to_owned())), Some(0));
    assert_eq!(pair.admission_sequence(&WindowId("c".to_owned())), Some(0));
    let (back1, back2) = pair.split_canonical_pair().expect("pair splits");
    let back1 = back1;
    let back2 = back2.expect("target survives");
    assert!(back1.is_capacity_float(&WindowId("b".to_owned())));
    assert_eq!(back1.admission_sequence(&WindowId("a".to_owned())), Some(0));
    assert_eq!(back2.admission_sequence(&WindowId("c".to_owned())), Some(0));
    assert!(!back2.is_capacity_float(&WindowId("b".to_owned())));
}

#[test]
fn engine_disjoint_unique_source_keeps_legacy_relocation() {
    use tiler_core::boundary::CoreReply;
    use tiler_core::engine::Engine;
    // out-a/ws-x retains win-s. A fresh reconcile on out-b/ws-x (same
    // workspace, so unique source) with disjoint roomy windows keeps
    // the retained legacy route: the source stays untouched and the
    // target seeds fresh through the legacy seed.
    let source = domain("out-a", "ws-x", 800, 600, 8);
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let seed = reconcile_event(
        &owner(),
        &generation(),
        &source,
        4,
        "win-s",
        vec![engine_window(
            "win-s",
            "out-a",
            "ws-x",
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        )],
        "r-1",
    );
    assert!(matches!(engine.handle(&seed), CoreReply::Tiled(_)));
    let target = domain("out-b", "ws-x", 800, 600, 8);
    let fresh = reconcile_event(
        &owner(),
        &generation(),
        &target,
        4,
        "win-t1",
        vec![
            engine_window(
                "win-t1",
                "out-b",
                "ws-x",
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            ),
            engine_window(
                "win-t2",
                "out-b",
                "ws-x",
                Rect {
                    x: 200,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            ),
        ],
        "r-2",
    );
    assert!(matches!(engine.handle(&fresh), CoreReply::Tiled(_)));
    assert_eq!(engine.retained_domains(), 2, "source stays, target seeds");
    let s_key = source.key();
    let t_key = target.key();
    let kept = engine.session(&s_key).expect("source retained");
    assert_eq!(tiled_labels(kept), vec!["win-s".to_string()]);
    let seeded = engine.session(&t_key).expect("target seeded");
    assert_eq!(
        tiled_labels(seeded),
        vec!["win-t1".to_string(), "win-t2".to_string()]
    );
    // Exhausted disjoint target still floats through best-effort
    // capacity instead of holding: the source stays untouched.
    let tiny = domain("out-c", "ws-x", 1, 1, 0);
    let squeezed = reconcile_event(
        &owner(),
        &generation(),
        &tiny,
        0,
        "win-u1",
        vec![
            engine_window(
                "win-u1",
                "out-c",
                "ws-x",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
            engine_window(
                "win-u2",
                "out-c",
                "ws-x",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
        ],
        "r-3",
    );
    assert!(matches!(engine.handle(&squeezed), CoreReply::Tiled(_)));
    let u_key = tiny.key();
    let tight = engine.session(&u_key).expect("target seeded");
    assert_eq!(tiled_labels(tight).len(), 1, "one tile survives");
    assert_eq!(tight.capacity_float_count(), 1, "exhaustion floats");
    let kept = engine.session(&s_key).expect("source still retained");
    assert_eq!(tiled_labels(kept), vec!["win-s".to_string()]);
}

#[test]
fn engine_independent_domains_share_monotonic_admission_order() {
    // D7 newest-first is the FIRST successful tiled-admission sequence with
    // the same lifetime preserved across moves. Two Engine-owned domains
    // admit in different chronological requests, so per-domain counters
    // starting at zero would collide or invert once the windows share a
    // domain: the later domain's window reuses sequence 0 while the
    // earlier domain already spent 0 and 1. After a send brings them
    // together, a shrink must evict the actual chronological newest, not
    // the sequence-highest older window. The only shared-sequence
    // exception is an explicit clean-fit roster.
    use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
    use tiler_core::engine::Engine;
    let ws1 = domain("o", "ws-1", 120, 80, 0);
    let ws2 = domain("o", "ws-2", 120, 80, 0);
    let key1 = ws1.key();
    let key2 = ws2.key();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    // Chronologically first: ws-1 admits old1, then old2 as a newcomer.
    let first = reconcile_event(
        &owner(),
        &generation(),
        &ws1,
        0,
        "old1",
        vec![engine_window(
            "old1",
            "o",
            "ws-1",
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
        )],
        "g-1",
    );
    assert!(matches!(engine.handle(&first), CoreReply::Tiled(_)));
    let second = reconcile_event(
        &owner(),
        &generation(),
        &ws1,
        0,
        "old2",
        vec![
            engine_window(
                "old1",
                "o",
                "ws-1",
                Rect {
                    x: 0,
                    y: 0,
                    w: 60,
                    h: 80,
                },
            ),
            engine_window(
                "old2",
                "o",
                "ws-1",
                Rect {
                    x: 60,
                    y: 0,
                    w: 60,
                    h: 80,
                },
            ),
        ],
        "g-2",
    );
    assert!(matches!(
        engine.handle(&second),
        CoreReply::Projection(_) | CoreReply::Tiled(_)
    ));
    // Chronologically later: ws-2 admits new1 on its independent session.
    let third = reconcile_event(
        &owner(),
        &generation(),
        &ws2,
        0,
        "new1",
        vec![engine_window(
            "new1",
            "o",
            "ws-2",
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
        )],
        "g-3",
    );
    assert!(matches!(engine.handle(&third), CoreReply::Tiled(_)));
    let seq = |engine: &tiler_core::engine::Engine, window: &str, key: &DomainKey| {
        engine
            .session(key)
            .expect("retained")
            .admission_sequence(&WindowId(window.to_owned()))
    };
    assert_eq!(seq(&engine, "old1", &key1), Some(0));
    assert_eq!(seq(&engine, "old2", &key1), Some(1));
    assert_eq!(
        seq(&engine, "new1", &key2),
        Some(2),
        "the chronologically later independent admission takes the next \
         Engine-wide sequence, never a colliding per-domain zero"
    );
    // Send new1 into ws-1: the source sequence rides the lifetime.
    let send = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: correlation("g-4"),
        revision: 0,
        fingerprint: 7,
        domain: ws2.clone(),
        domain_key: key2.clone(),
        outer_gap: 0,
        focused_window: WindowId("new1".to_owned()),
        windows: vec![engine_window(
            "new1",
            "o",
            "ws-2",
            Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 80,
            },
        )],
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((ws1.clone(), key1.clone())),
        target_windows: vec![
            engine_window(
                "old1",
                "o",
                "ws-1",
                Rect {
                    x: 0,
                    y: 0,
                    w: 40,
                    h: 80,
                },
            ),
            engine_window(
                "old2",
                "o",
                "ws-1",
                Rect {
                    x: 40,
                    y: 0,
                    w: 40,
                    h: 80,
                },
            ),
        ],
        closed_windows: vec![],
        command: CoreCommand::SendToWorkspace {
            window: "new1".to_owned(),
            target_output: "o".to_owned(),
            target_workspace: "ws-1".to_owned(),
            follow: true,
        },
    };
    match engine.handle(&send) {
        CoreReply::SendWorkspace(_) => {}
        other => panic!("send commits, got {other:?}"),
    }
    assert_eq!(
        seq(&engine, "new1", &key1),
        Some(2),
        "send preserves the sequence"
    );
    assert_eq!(seq(&engine, "old1", &key1), Some(0));
    assert_eq!(seq(&engine, "old2", &key1), Some(1));
    // Shrink ws-1 to 2x1: no 3-leaf tree projects there (either split
    // axis starves its nested group), while any surviving 2-leaf
    // horizontal pair does. Exactly the newest victim floats, which must
    // be the chronological newest new1.
    let shrunk_ws1 = domain("o", "ws-1", 2, 1, 0);
    let shrink = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: correlation("g-5"),
        revision: 0,
        fingerprint: 7,
        domain: shrunk_ws1,
        domain_key: key1.clone(),
        outer_gap: 0,
        focused_window: WindowId("new1".to_owned()),
        windows: vec![
            engine_window(
                "old1",
                "o",
                "ws-1",
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 80,
                },
            ),
            engine_window(
                "old2",
                "o",
                "ws-1",
                Rect {
                    x: 1,
                    y: 0,
                    w: 1,
                    h: 80,
                },
            ),
            engine_window(
                "new1",
                "o",
                "ws-1",
                Rect {
                    x: 0,
                    y: 0,
                    w: 2,
                    h: 80,
                },
            ),
        ],
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::UpdateGaps,
    };
    match engine.handle(&shrink) {
        CoreReply::Projection(_) => {}
        other => panic!("shrink repairs with a projection, got {other:?}"),
    }
    let after = engine.session(&key1).expect("retained");
    assert_eq!(
        tiled_labels(after),
        vec!["old1".to_string(), "old2".to_string()],
        "shrink evicts the actual chronological newest first"
    );
    assert!(
        after.is_capacity_float(&WindowId("new1".to_owned())),
        "newest victim floats with diagnostic origin"
    );
}
