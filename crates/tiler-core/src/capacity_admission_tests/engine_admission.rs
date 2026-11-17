//! Engine-side capacity admission: fresh mixed seeds, newcomer projection and marks, ordinary-float parity, echo persistence, and order.
//!
//! Shared helpers live in the parent (`capacity_admission_tests.rs`); this module
//! holds only `#[test]` fns.

use super::*;
use crate::session::{ExceptionBehavior, RefusalKind};
use crate::world_layout::{
    FreshOutcomeKind, LayoutFailure, LocalOperationContext, LocalOperationFailure,
    PairOperationContext,
};

#[test]
fn engine_fresh_mixed_seed_800x600_gap8_commits_complete_roster() {
    // Fourteen overlapping 200x200 observations in 800x600 gap 8: the
    // clean fit declines (centre splits), so the spatial focus-last
    // serial seed commits one atomic roster instead of discarding the
    // whole seed. Largest-fallback packing tiles all fourteen here;
    // smaller geometries exercise the floated half below.
    let d = domain("out-1", "ws1", 800, 600, 8);
    let windows: Vec<EngineWindow> = (0..14)
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
    let mut layout = WorldLayout::new();
    let out = layout
        .reconcile_fresh(&d, 4, &WindowId(String::new()), &windows)
        .expect("fresh mixed seed commits");
    assert_eq!(out.kind, FreshOutcomeKind::SerialSeed);
    let key = d.key();
    let mut live: Vec<String> = layout
        .tiled_windows(&key)
        .chain(layout.exception_windows(&key))
        .map(|w| w.0.clone())
        .collect();
    live.sort();
    let expected: Vec<String> = (0..14).map(|i: i32| format!("win-{i:06}")).collect();
    assert_eq!(live, expected, "every lifetime keeps exactly one row");
    assert_eq!(
        layout.accepted_revision(&key),
        1,
        "one atomic commit for the whole roster"
    );
    // Stable replay: the same report converges exactly.
    let revision = layout.accepted_revision(&key);
    layout
        .converge_retained_observation(&key, &windows, &WindowId(String::new()), &[])
        .expect("replay converges");
    layout
        .reconcile_retained(
            &key,
            d.bounds,
            d.gap,
            4,
            &WindowId(String::new()),
            &windows,
            &[],
        )
        .expect("replay reconciles");
    assert_eq!(
        layout.accepted_revision(&key),
        revision,
        "replay advances nothing"
    );
}

#[test]
fn engine_small_domain_seed_mixes_tiles_and_capacity_floats() {
    // 10x10 gap 8 exhausts after four tiles: the last two legs
    // capacity-float in the same atomic commit (revision 1).
    let d = domain("o", "w", 10, 10, 8);
    let windows: Vec<EngineWindow> = (0..6)
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
    let mut layout = WorldLayout::new();
    let out = layout
        .reconcile_fresh(&d, 0, &WindowId(String::new()), &windows)
        .expect("mixed roster commits");
    assert_eq!(out.kind, FreshOutcomeKind::SerialSeed);
    let key = d.key();
    assert_eq!(layout.tiled_count(&key), 4, "four tiles");
    assert_eq!(layout.exception_count(&key), 2, "two capacity floats");
    assert_eq!(layout.accepted_revision(&key), 1, "one atomic commit");

    // The focused-last anchor floats: the outcome names it as the native
    // float target with no frame authority, while the tiled remainder
    // still projects.
    let mut layout = WorldLayout::new();
    let out = layout
        .reconcile_fresh(&d, 0, &WindowId("win-000005".to_owned()), &windows)
        .expect("anchor-float seed admits mixed roster");
    assert_eq!(
        out.native_capacity_float,
        Some(WindowId("win-000005".to_owned())),
        "floated anchor is the native float target"
    );
    assert_eq!(
        out.projection.geometry.len(),
        4,
        "tiled remainder still projects"
    );
    assert!(layout.is_capacity_float(&key, &WindowId("win-000005".to_owned())));
}

#[test]
fn engine_capacity_float_newcomer_keeps_projection_and_marks() {
    let d = domain("tiny", "ws", 1, 1, 0);
    let key = d.key();
    let mut layout = WorldLayout::new();
    layout
        .reconcile_fresh(
            &d,
            0,
            &WindowId("a".to_owned()),
            &[engine_window(
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
        )
        .expect("first commits");
    // Newcomer b exhausts the retained 1x1 domain: observation
    // convergence capacity-floats it at its native frame with no tile
    // slot, and the reconcile projects the intact survivor.
    let windows = vec![
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
    ];
    layout
        .converge_retained_observation(&key, &windows, &WindowId("a".to_owned()), &[])
        .expect("newcomer converges");
    let plan = layout
        .reconcile_retained(
            &key,
            d.bounds,
            d.gap,
            0,
            &WindowId("a".to_owned()),
            &windows,
            &[],
        )
        .expect("survivor reconciles")
        .expect("retained");
    assert_eq!(plan.geometry.len(), 1);
    assert_eq!(plan.geometry[0].window.0, "a");
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));
    assert_eq!(tiled_labels(&layout, &key), vec!["a".to_string()]);
}

#[test]
fn capacity_float_matches_ordinary_float_with_no_auto_retile() {
    // D4 diagnostic-origin-only: a capacity float behaves exactly like an
    // ordinary float under equivalent reports. A normalized floating:true
    // echo persists with no auto-retile (no counts, no revision) across
    // replay, growth, and gap changes; growth alone never tiles. A raw
    // floating:false echo means explicit external intent per the ordinary
    // observation authority: convergence re-attempts admission exactly as
    // for an ordinary float, and preconverged ops bind flags exactly.
    let d = domain("tiny", "ws", 1, 1, 0);
    let key = d.key();
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d, "a", placement(1, 1));
    let obs = complete_observed_for(&layout, &key, vec![tiled_observed("b", "tiny", "ws")]);
    layout
        .admit_window(
            &key,
            &WindowId("b".to_owned()),
            ExceptionFlags::none(),
            None,
            placement(1, 1),
            false,
            false,
            &obs,
        )
        .expect("b floats");
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));

    // Normalized echo on replay: still floated, revision untouched.
    let revision = layout.accepted_revision(&key);
    let echo = complete_observed_for(&layout, &key, Vec::new());
    assert!(
        echo.iter()
            .find(|w| w.window.0 == "b")
            .is_some_and(|w| w.floating),
        "normalized echo reports the float as floating"
    );
    let counts = layout
        .converge_domain(
            &key,
            &echo,
            Some(&WindowId("a".to_owned())),
            &BTreeSet::new(),
        )
        .expect("echo converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(
        layout.accepted_revision(&key),
        revision,
        "no silent re-tile"
    );
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));

    // Growth alone never re-tiles under the normalized echo.
    let grown = domain("tiny", "ws", 100, 100, 0);
    let eng_echo = complete_engine_obs(&layout, &[&key]);
    layout
        .update_domain_gaps(
            &key,
            grown.bounds,
            0,
            0,
            &WindowId("a".to_owned()),
            &eng_echo,
        )
        .expect("growth applies");
    let echo = complete_observed_for(&layout, &key, Vec::new());
    let counts = layout
        .converge_domain(
            &key,
            &echo,
            Some(&WindowId("a".to_owned())),
            &BTreeSet::new(),
        )
        .expect("grown echo converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert!(
        layout.is_capacity_float(&key, &WindowId("b".to_owned())),
        "growth alone never re-tiles"
    );
    assert_eq!(tiled_labels(&layout, &key), vec!["a".to_string()]);

    // Ordinary-float equivalence under the same normalized report: an
    // ordinary deferred float converges identically (zero counts).
    let mut ordinary = WorldLayout::new();
    admit_into(&mut ordinary, &grown, "a", placement(1, 1));
    {
        let mut extra = tiled_observed("b", "tiny", "ws");
        extra.floating = true;
        let obs = complete_observed_for(&ordinary, &key, vec![extra]);
        ordinary
            .admit_window(
                &key,
                &WindowId("b".to_owned()),
                ExceptionFlags {
                    floating: true,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                },
                Some(ExceptionBehavior::Defer),
                placement(1, 1),
                false,
                false,
                &obs,
            )
            .expect("ordinary float defers");
    }
    assert!(!ordinary.is_capacity_float(&key, &WindowId("b".to_owned())));
    let ordinary_echo = complete_observed_for(&ordinary, &key, Vec::new());
    let ordinary_counts = ordinary
        .converge_domain(
            &key,
            &ordinary_echo,
            Some(&WindowId("a".to_owned())),
            &BTreeSet::new(),
        )
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

    // Raw floating:false means explicit external intent: convergence
    // re-attempts admission instead of silently ignoring.
    let false_cap = echo_observed_for(&layout, &key, &[("b", false)]);
    assert!(
        false_cap
            .iter()
            .find(|w| w.window.0 == "b")
            .is_some_and(|w| !w.floating)
    );
    let cap_counts = layout
        .converge_domain(
            &key,
            &false_cap,
            Some(&WindowId("a".to_owned())),
            &BTreeSet::new(),
        )
        .expect("false echo converges like ordinary intent");
    assert!(
        cap_counts.flags_adopted > 0 || cap_counts.admitted > 0,
        "false echo re-attempts admission, never silently ignored: {cap_counts:?}"
    );

    // Preconverged paths bind exactly: an op under a stale false echo
    // refuses like an ordinary float mismatch (PartialObservation).
    let mut layout2 = WorldLayout::new();
    admit_into(&mut layout2, &d, "a", placement(1, 1));
    let obs = complete_observed_for(&layout2, &key, vec![tiled_observed("b", "tiny", "ws")]);
    layout2
        .admit_window(
            &key,
            &WindowId("b".to_owned()),
            ExceptionFlags::none(),
            None,
            placement(1, 1),
            false,
            false,
            &obs,
        )
        .expect("b floats");
    let mut stale_eng = complete_engine_obs(&layout2, &[&key]);
    for entry in &mut stale_eng {
        if entry.window.0 == "b" {
            entry.floating = false;
        }
    }
    let ctx = LocalOperationContext {
        domain: &d,
        outer_gap: 0,
        observed_focus: &WindowId("a".to_owned()),
        observed: &stale_eng,
        preconverged: true,
    };
    assert_eq!(
        layout2.run_toggle_float(&ctx, &WindowId("a".to_owned()), None),
        Err(LocalOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::PartialObservation
        )))
    );

    // Explicit ToggleFloat under the normalized echo tiles b once room
    // exists and clears the origin with no false fixed-tile override.
    let eng_echo = complete_engine_obs(&layout2, &[&key]);
    layout2
        .update_domain_gaps(
            &key,
            grown.bounds,
            0,
            0,
            &WindowId("a".to_owned()),
            &eng_echo,
        )
        .expect("growth applies");
    let eng_echo = complete_engine_obs(&layout2, &[&key]);
    let ctx = LocalOperationContext {
        domain: &grown,
        outer_gap: 0,
        observed_focus: &WindowId("b".to_owned()),
        observed: &eng_echo,
        preconverged: true,
    };
    layout2
        .run_toggle_float(&ctx, &WindowId("b".to_owned()), None)
        .expect("explicit unfloat tiles under normalized echo");
    assert_eq!(
        tiled_labels(&layout2, &key),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(!layout2.is_capacity_float(&key, &WindowId("b".to_owned())));
}

#[test]
fn engine_capacity_float_persists_under_normalized_echo_until_toggle() {
    let tiny_dom = domain("tiny", "ws", 1, 1, 0);
    let key = tiny_dom.key();
    let mut layout = WorldLayout::new();
    layout
        .reconcile_fresh(
            &tiny_dom,
            0,
            &WindowId("a".to_owned()),
            &[engine_window(
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
        )
        .expect("first commits");
    let second_windows = vec![
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
    ];
    layout
        .converge_retained_observation(&key, &second_windows, &WindowId("a".to_owned()), &[])
        .expect("second converges");
    layout
        .reconcile_retained(
            &key,
            tiny_dom.bounds,
            tiny_dom.gap,
            0,
            &WindowId("a".to_owned()),
            &second_windows,
            &[],
        )
        .expect("second reconciles");
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));

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
    let third_windows = vec![
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
        floated_b.clone(),
    ];
    layout
        .converge_retained_observation(&key, &third_windows, &WindowId("a".to_owned()), &[])
        .expect("third converges");
    let plan = layout
        .reconcile_retained(
            &key,
            grown.bounds,
            grown.gap,
            0,
            &WindowId("a".to_owned()),
            &third_windows,
            &[],
        )
        .expect("grown survivor projects")
        .expect("retained");
    assert_eq!(plan.geometry.len(), 1);
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));

    // Explicit toggle-float (observed floating:true, like an ordinary
    // float unfloat) tiles b once room exists.
    let toggle_windows = vec![
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
        floated_b,
    ];
    let ctx = LocalOperationContext {
        domain: &grown,
        outer_gap: 0,
        observed_focus: &WindowId("b".to_owned()),
        observed: &toggle_windows,
        preconverged: false,
    };
    let out = layout
        .run_toggle_float(&ctx, &WindowId("b".to_owned()), None)
        .expect("explicit toggle tiles");
    assert!(
        out.geometry.iter().any(|g| g.window.0 == "b"),
        "explicit toggle tiles b: {out:?}"
    );
    assert!(!layout.is_capacity_float(&key, &WindowId("b".to_owned())));
    assert!(layout.is_tiled_window(&key, &WindowId("b".to_owned())));
}

#[test]
fn capacity_float_preserves_focus_like_ordinary_float() {
    // Diagnostic origin grants no extra behavior and suppresses none:
    // a capacity-floated newcomer under observed focus leaves the tiled
    // MRU exactly where an ordinary deferred float arrival does.
    let d = domain("tiny", "ws", 1, 1, 0);
    let key = d.key();
    for floated in ["cap", "ord"] {
        let mut layout = WorldLayout::new();
        admit_into(&mut layout, &d, "a", placement(1, 1));
        if floated == "cap" {
            let obs = complete_observed_for(&layout, &key, vec![tiled_observed("b", "tiny", "ws")]);
            layout
                .admit_window(
                    &key,
                    &WindowId("b".to_owned()),
                    ExceptionFlags::none(),
                    None,
                    placement(1, 1),
                    false,
                    false,
                    &obs,
                )
                .expect("capacity floats");
            assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));
        } else {
            let mut extra = tiled_observed("b", "tiny", "ws");
            extra.floating = true;
            let obs = complete_observed_for(&layout, &key, vec![extra]);
            layout
                .admit_window(
                    &key,
                    &WindowId("b".to_owned()),
                    ExceptionFlags {
                        floating: true,
                        fullscreen: false,
                        maximized: false,
                        sticky: false,
                    },
                    Some(ExceptionBehavior::Defer),
                    placement(1, 1),
                    false,
                    false,
                    &obs,
                )
                .expect("ordinary float defers");
            assert!(!layout.is_capacity_float(&key, &WindowId("b".to_owned())));
        }
        assert_eq!(
            layout.focus_leaf(&key),
            Some(&leaf_of(&layout, &key, "a")),
            "{floated}: tiled MRU preserved identically"
        );
    }
}

#[test]
fn pair_round_trip_preserves_capacity_order_globally() {
    // Admission sequence and capacity origin survive
    // pair operations partitioned by homing across domains in one WorldLayout.
    let d1 = domain("o", "ws-1", 1, 1, 0);
    let d2 = domain("o", "ws-2", 100, 100, 0);
    let key1 = d1.key();
    let key2 = d2.key();
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d1, "a", placement(1, 1));
    let obs = complete_observed_for(&layout, &key1, vec![tiled_observed("b", "o", "ws-1")]);
    layout
        .admit_window(
            &key1,
            &WindowId("b".to_owned()),
            ExceptionFlags::none(),
            None,
            placement(1, 1),
            false,
            false,
            &obs,
        )
        .expect("b floats");
    assert!(layout.is_capacity_float(&key1, &WindowId("b".to_owned())));
    admit_into(&mut layout, &d2, "c", placement(10, 10));
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("a".to_owned())),
        Some(0)
    );
    assert_eq!(
        layout.admission_sequence(&key2, &WindowId("c".to_owned())),
        Some(1)
    );

    // Send b (capacity float) from ws-1 to ws-2, then back to ws-1:
    // capacity origin and sequence partitioning stay intact.
    let obs = complete_engine_obs(&layout, &[&key1, &key2]);
    let ctx1 = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx1, &WindowId("b".to_owned()), false, false)
        .expect("send b to ws-2");
    assert!(layout.is_capacity_float(&key2, &WindowId("b".to_owned())));
    assert!(!layout.is_capacity_float(&key1, &WindowId("b".to_owned())));
    let obs = complete_engine_obs(&layout, &[&key1, &key2]);
    let ctx2 = PairOperationContext {
        source_domain: &d2,
        source_outer_gap: 0,
        target_domain: &d1,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx2, &WindowId("b".to_owned()), false, false)
        .expect("send b back to ws-1");
    assert!(layout.is_capacity_float(&key1, &WindowId("b".to_owned())));
    assert!(!layout.is_capacity_float(&key2, &WindowId("b".to_owned())));
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("a".to_owned())),
        Some(0)
    );
    assert_eq!(
        layout.admission_sequence(&key2, &WindowId("c".to_owned())),
        Some(1)
    );
}

#[test]
fn engine_disjoint_unique_source_keeps_legacy_relocation() {
    // out-a/ws-x retains win-s. A fresh reconcile on out-b/ws-x (same
    // workspace, so unique source) with disjoint roomy windows keeps
    // the retained legacy route: the source stays untouched and the
    // target seeds fresh through the legacy seed.
    let source = domain("out-a", "ws-x", 800, 600, 8);
    let mut layout = WorldLayout::new();
    layout
        .reconcile_fresh(
            &source,
            4,
            &WindowId("win-s".to_owned()),
            &[engine_window(
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
        )
        .expect("source seeds");
    let target = domain("out-b", "ws-x", 800, 600, 8);
    layout
        .reconcile_fresh(
            &target,
            4,
            &WindowId("win-t1".to_owned()),
            &[
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
        )
        .expect("target seeds");
    assert_eq!(layout.domain_keys().len(), 2, "source stays, target seeds");
    let s_key = source.key();
    let t_key = target.key();
    assert_eq!(tiled_labels(&layout, &s_key), vec!["win-s".to_string()]);
    assert_eq!(
        tiled_labels(&layout, &t_key),
        vec!["win-t1".to_string(), "win-t2".to_string()]
    );
    // Exhausted disjoint target still floats through best-effort
    // capacity instead of holding: the source stays untouched.
    let tiny = domain("out-c", "ws-x", 1, 1, 0);
    layout
        .reconcile_fresh(
            &tiny,
            0,
            &WindowId("win-u1".to_owned()),
            &[
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
        )
        .expect("tiny seeds");
    let u_key = tiny.key();
    assert_eq!(tiled_labels(&layout, &u_key).len(), 1, "one tile survives");
    assert_eq!(layout.exception_count(&u_key), 1, "exhaustion floats");
    assert_eq!(tiled_labels(&layout, &s_key), vec!["win-s".to_string()]);
}

#[test]
fn engine_independent_domains_share_monotonic_admission_order() {
    // D7 newest-first is the FIRST successful tiled-admission sequence with
    // the same lifetime preserved across moves. Two WorldLayout domains
    // admit in different chronological requests; a shared high-water
    // counter ensures they never collide, so after a send brings them
    // together, a shrink evicts the actual chronological newest.
    let ws1 = domain("o", "ws-1", 120, 80, 0);
    let ws2 = domain("o", "ws-2", 120, 80, 0);
    let key1 = ws1.key();
    let key2 = ws2.key();
    let mut layout = WorldLayout::new();
    // Chronologically first: ws-1 admits old1, then old2 as a newcomer.
    layout
        .reconcile_fresh(
            &ws1,
            0,
            &WindowId("old1".to_owned()),
            &[engine_window(
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
        )
        .expect("old1 seeds");
    let second_windows = vec![
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
    ];
    layout
        .converge_retained_observation(&key1, &second_windows, &WindowId("old2".to_owned()), &[])
        .expect("old2 converges");
    // Chronologically later: ws-2 admits new1 on its independent domain.
    layout
        .reconcile_fresh(
            &ws2,
            0,
            &WindowId("new1".to_owned()),
            &[engine_window(
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
        )
        .expect("new1 seeds");
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("old1".to_owned())),
        Some(0)
    );
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("old2".to_owned())),
        Some(1)
    );
    assert_eq!(
        layout.admission_sequence(&key2, &WindowId("new1".to_owned())),
        Some(2),
        "the chronologically later independent admission takes the next \
         WorldLayout-wide sequence, never a colliding per-domain zero"
    );
    // Send new1 into ws-1: the source sequence rides the lifetime.
    let send_obs = vec![
        engine_window(
            "new1",
            "o",
            "ws-2",
            Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 80,
            },
        ),
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
    ];
    let ctx = PairOperationContext {
        source_domain: &ws2,
        source_outer_gap: 0,
        target_domain: &ws1,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("new1".to_owned()),
        observed: &send_obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx, &WindowId("new1".to_owned()), true, false)
        .expect("send commits");
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("new1".to_owned())),
        Some(2),
        "send preserves the sequence"
    );
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("old1".to_owned())),
        Some(0)
    );
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("old2".to_owned())),
        Some(1)
    );
    // Shrink ws-1 to 2x1: no 3-leaf tree projects there, while any
    // surviving 2-leaf horizontal pair does. Exactly the newest victim
    // floats, which must be the chronological newest new1.
    let shrink_obs = vec![
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
    ];
    layout
        .update_domain_gaps(
            &key1,
            Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            },
            0,
            0,
            &WindowId("new1".to_owned()),
            &shrink_obs,
        )
        .expect("shrink repairs with a projection");
    assert_eq!(
        tiled_labels(&layout, &key1),
        vec!["old1".to_string(), "old2".to_string()],
        "shrink evicts the actual chronological newest first"
    );
    assert!(
        layout.is_capacity_float(&key1, &WindowId("new1".to_owned())),
        "newest victim floats with diagnostic origin"
    );
}
