//! D2/D3/D7 admission policy over `WorldLayout`: defaults, fallback ranking,
//! exhaustion/retry/eviction stability, and no settings reflow.
//!
//! Shared helpers live in the parent (`capacity_admission_tests.rs`); this module
//! holds only `#[test]` fns.

use super::*;
use crate::world_layout::WorldLayout;

#[test]
fn largest_fallback_tiles_with_dfs_tiebreak() {
    // 12x12 gap 4: H[A,B] halves of 4x12. A wide-placed newcomer fails
    // the ordinary horizontal wrap of B (4 - 4 < 2), then the largest
    // fallback splits A: equal 4x12 areas tie, DFS order picks A first.
    let d = domain("o", "w", 12, 12, 4);
    let key = d.key();
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d, "a", placement(12, 4));
    admit_into(&mut layout, &d, "b", placement(12, 4));
    assert_eq!(layout.focus_leaf(&key), Some(&leaf_of(&layout, &key, "b")));
    let geometry = admit_into(&mut layout, &d, "c", placement(12, 4));
    assert_eq!(geometry.len(), 3);
    assert!(
        geometry.iter().all(|g| g.rect.w > 0 && g.rect.h > 0),
        "every tile stays positive"
    );
    let tree = layout.tree(&key).expect("tree").clone();
    let c_leaf = leaf_of(&layout, &key, "c");
    let siblings = siblings_of(&tree, &c_leaf).expect("c has leaf siblings");
    assert!(
        siblings.contains(&leaf_of(&layout, &key, "a")),
        "DFS tiebreak splits A, not the ordinary anchor B: {siblings:?}"
    );
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&c_leaf),
        "successful admission focuses the newcomer"
    );
    // First-successful-tiled-admission order tracks a, b, c.
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("a".to_owned())),
        Some(0)
    );
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("b".to_owned())),
        Some(1)
    );
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("c".to_owned())),
        Some(2)
    );
}

#[test]
fn b6_minimum_overlap_never_blocks_search_or_float() {
    // Declared minima alone never count as exhaustion (D1): an
    // unsatisfiable minimum keeps its proportional tile flagged
    // overconstrained, and later admissions still tile.
    let d = domain("o", "w", 100, 100, 0);
    let key = d.key();
    let mut layout = WorldLayout::new();
    let mut a_entry = engine_window("a", "o", "w", placement(100, 100));
    a_entry.hints = WindowSizeHints {
        min_w: Some(200),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    layout
        .reconcile_fresh(&d, 0, &WindowId("a".to_owned()), &[a_entry])
        .expect("single leaf always tiles");
    assert!(!layout.is_exception(&key, &WindowId("a".to_owned())));
    admit_into(&mut layout, &d, "b", placement(100, 100));
    assert_eq!(
        tiled_windows(&layout),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(
        !layout.is_capacity_float(&key, &WindowId("b".to_owned())),
        "minimum overlap alone never floats"
    );
}

#[test]
fn one_by_one_exhaustion_floats_with_stable_replay_and_close_clear() {
    let d = domain("tiny", "ws", 1, 1, 0);
    let key = d.key();
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d, "a", placement(1, 1));
    let obs = complete_observed_for(&layout, &key, vec![tiled_observed("b", "tiny", "ws")]);
    let base = layout.accepted_revision(&key);
    let outcome = layout
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
        .expect("1x1 exhausts to float");
    assert!(outcome.geometry.is_empty());
    assert_eq!(layout.accepted_revision(&key), base + 1);
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));
    assert!(
        !layout.is_automatic_fixed_float(&key, &WindowId("b".to_owned())),
        "capacity origin stays distinct from fixed auto"
    );
    // Stable replay: the identical accepted observation converges
    // exactly with no new revision, focus change, or re-tile.
    let replay = complete_observed_for(&layout, &key, Vec::new());
    let revision = layout.accepted_revision(&key);
    let focus = layout.focus_leaf(&key).cloned();
    let counts = layout
        .converge_domain(
            &key,
            &replay,
            Some(&WindowId("a".to_owned())),
            &BTreeSet::new(),
        )
        .expect("replay converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(layout.accepted_revision(&key), revision);
    assert_eq!(layout.focus_leaf(&key).cloned(), focus);
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));
    // Close clears origin and order (D5).
    let close_obs: Vec<ObservedWindow> = complete_observed_for(&layout, &key, Vec::new())
        .into_iter()
        .filter(|w| w.window.0 != "b")
        .collect();
    layout
        .converge_domain(
            &key,
            &close_obs,
            Some(&WindowId("a".to_owned())),
            &BTreeSet::new(),
        )
        .expect("close converges");
    assert!(!layout.is_capacity_float(&key, &WindowId("b".to_owned())));
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("b".to_owned())),
        None
    );
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("a".to_owned())),
        Some(0)
    );
}

#[test]
fn explicit_unfloat_retries_once_without_false_override() {
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
        .expect("floats");
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));
    // Explicit unfloat retries once against still-insufficient space:
    // b stays floated with capacity origin and no tile override.
    assert!(layout.is_exception(&key, &WindowId("b".to_owned())));
    let obs = complete_observed_for(&layout, &key, Vec::new());
    let base = layout.accepted_revision(&key);
    let outcome = layout
        .toggle_float(&key, &WindowId("b".to_owned()), None, &obs)
        .expect("retry succeeds as capacity no-space re-float");
    assert_eq!(
        outcome.float_window,
        Some(WindowId("b".to_owned())),
        "re-float reports no-space target"
    );
    assert_eq!(outcome.float_rect, None);
    assert_eq!(layout.accepted_revision(&key), base + 1);
    assert!(
        layout.is_capacity_float(&key, &WindowId("b".to_owned())),
        "stays floated"
    );
    assert!(
        !layout.has_fixed_tile_override(&key, &WindowId("b".to_owned())),
        "no false fixed-tile override on a non-fixed re-float"
    );
    assert_eq!(tiled_windows(&layout), vec!["a".to_string()]);
}

#[test]
fn shrink_evicts_newest_first_with_protections_and_mru_focus() {
    // Settle a/b/c in 120x80 gap 0, then shrink to 2x80: H[a,H[b,c]]
    // needs width 3, so the newest victim (c) floats alone.
    let d = domain("o", "w", 120, 80, 0);
    let key = d.key();
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d, "a", placement(120, 40));
    admit_into(&mut layout, &d, "b", placement(120, 40));
    admit_into(&mut layout, &d, "c", placement(120, 40));
    let live_rect = Rect {
        x: 1,
        y: 2,
        w: 30,
        h: 30,
    };
    let live_obs: Vec<EngineWindow> = ["a", "b", "c"]
        .into_iter()
        .map(|w| engine_window(w, "o", "w", live_rect))
        .collect();
    layout
        .update_domain_gaps(
            &key,
            Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 80,
            },
            0,
            0,
            &WindowId("c".to_owned()),
            &live_obs,
        )
        .expect("shrink repairs c to float");
    assert_eq!(
        tiled_windows(&layout),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(layout.is_capacity_float(&key, &WindowId("c".to_owned())));
    assert_eq!(
        layout.retained_float_geometry(&key, &WindowId("c".to_owned())),
        Some(live_rect),
        "eviction keeps the last live frame"
    );
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "b")),
        "tiled focus falls back to the surviving MRU, never the floated window"
    );
    // An explicit fixed-tile override protects c: the next victim is b.
    let mut guarded = WorldLayout::new();
    admit_into(&mut guarded, &d, "a", placement(120, 40));
    admit_into(&mut guarded, &d, "b", placement(120, 40));
    // Admit c with an explicit user tile win (suppress_fixed_float = true).
    let obs = complete_observed_for(&guarded, &key, vec![tiled_observed("c", "o", "w")]);
    guarded
        .admit_window(
            &key,
            &WindowId("c".to_owned()),
            ExceptionFlags::none(),
            None,
            placement(120, 40),
            true,
            false,
            &obs,
        )
        .expect("c tiles");
    assert!(guarded.has_fixed_tile_override(&key, &WindowId("c".to_owned())));
    guarded
        .update_domain_gaps(
            &key,
            Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 80,
            },
            0,
            0,
            &WindowId("c".to_owned()),
            &live_obs,
        )
        .expect("shrink repairs b to float while c is protected");
    assert_eq!(
        tiled_windows(&guarded),
        vec!["a".to_string(), "c".to_string()]
    );
    assert!(guarded.is_capacity_float(&key, &WindowId("b".to_owned())));
    assert!(
        !guarded.is_exception(&key, &WindowId("c".to_owned())),
        "explicit override never evicts"
    );
    // A protected-only impossible tree holds truthfully without mutation:
    // shrink until even the protected pair cannot project, then repair
    // with every survivor protected.
    let before_tree = guarded.tree(&key).cloned();
    let before_revision = guarded.accepted_revision(&key);
    let protected_obs: Vec<EngineWindow> = ["a", "c"]
        .into_iter()
        .map(|w| {
            let mut e = engine_window(w, "o", "w", live_rect);
            e.sticky = true;
            e
        })
        .chain(std::iter::once({
            let mut b = engine_window("b", "o", "w", live_rect);
            b.floating = true;
            b
        }))
        .collect();
    assert!(
        guarded
            .update_domain_gaps(
                &key,
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 80,
                },
                0,
                0,
                &WindowId("c".to_owned()),
                &protected_obs,
            )
            .is_err(),
        "protected-only impossible tree refuses without mutation"
    );
    assert_eq!(
        guarded.tree(&key),
        before_tree.as_ref(),
        "hold mutates nothing"
    );
    assert_eq!(guarded.accepted_revision(&key), before_revision);
    assert_eq!(
        guarded.admission_sequence(&key, &WindowId("a".to_owned())),
        Some(0)
    );
}
