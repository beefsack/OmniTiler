use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::size_hints::WindowSizeHints;

use super::*;

fn domain(output: &str, workspace: &str, w: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w,
            h: 100,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn window(id: &str, output: &str, workspace: &str, x: i32, w: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect { x, y: 0, w, h: 100 },
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

/// Port of the focused clean-fit and unfocused first-tile anchor cases from
/// `fresh_admit_shared`: a focused non-floating anchor takes the fit route
/// (base zero, revision one, shared sequence); an anchor that is not the
/// observed focus skips fit and takes the serial spatial seed with the anchor
/// admitted last and revision advanced exactly once.
#[test]
fn fresh_adoption_fit_focused_anchor_vs_serial_first_tile_anchor() {
    let d = domain("out", "ws", 300);
    let a = WindowId("a".to_owned());
    let b = WindowId("b".to_owned());
    let c = WindowId("c".to_owned());
    let roster = || {
        vec![
            window("a", "out", "ws", 0, 100),
            window("b", "out", "ws", 100, 100),
            window("c", "out", "ws", 200, 100),
        ]
    };

    // Focused anchor: clean fit commits at base zero / revision one.
    let mut fit_layout = WorldLayout::new();
    fit_layout.admission_seq_next = 5;
    let fit = fit_layout
        .reconcile_fresh(&d, 0, &b, &roster())
        .expect("focused clean fit adopts");
    assert_eq!(fit.kind, FreshOutcomeKind::Fit);
    assert_eq!(fit.projection.base_revision, 0);
    assert_eq!(fit.projection.geometry.len(), 3);
    let fit_stored = fit_layout.domain(&d.key()).expect("fit domain installed");
    assert_eq!(fit_stored.accepted_revision, 1);
    let fit_b_leaf = fit_stored.leaves.get(&b).expect("b bound");
    assert_eq!(fit.projection.focus_leaf, Some(fit_b_leaf.clone()));
    assert!(fit_stored.admission_order.values().all(|value| *value == 5));
    assert_eq!(fit_layout.admission_seq_next, 6);

    // Unfocused anchor: fit is skipped, the spatial seed admits the anchor
    // last and convergence advances the revision exactly once. Resolve every
    // leaf from this layout's own bindings; fitted and seeded leaf ids differ.
    let mut serial_layout = WorldLayout::new();
    let elsewhere = WindowId("elsewhere".to_owned());
    let serial = serial_layout
        .reconcile_fresh(&d, 0, &elsewhere, &roster())
        .expect("unfocused first-tile anchor seeds");
    assert_eq!(serial.kind, FreshOutcomeKind::SerialSeed);
    assert_eq!(serial.projection.base_revision, 1);
    assert_eq!(serial.projection.geometry.len(), 3);
    let serial_stored = serial_layout
        .domain(&d.key())
        .expect("seeded domain installed");
    assert_eq!(serial_stored.accepted_revision, 1);
    let sa = serial_stored.leaves.get(&a).expect("a bound").clone();
    let sb = serial_stored.leaves.get(&b).expect("b bound").clone();
    let sc = serial_stored.leaves.get(&c).expect("c bound").clone();
    assert_eq!(serial_stored.focus_stack, vec![sb, sc, sa.clone()]);
    assert_eq!(serial.projection.focus_leaf, Some(sa.clone()));
    assert_eq!(serial_stored.focused_leaf, Some(sa));
    assert!(serial_layout.group_alloc.next_value() > 0);
}

/// Port of `engine_disjoint_unique_source_keeps_legacy_relocation`: a unique
/// same-workspace source is tried for relocation, but a disjoint carried roster
/// fails the exact-set admit and rebuilds the legacy seed. The source, its
/// revision and its admission sequence survive; only the target installs.
#[test]
fn fresh_adoption_unique_source_rollback_rebuilds_seed_preserving_source() {
    let source_domain = domain("out", "ws", 300);
    let target_domain = domain("out-2", "ws", 800);
    let source_key = source_domain.key();
    let target_key = target_domain.key();
    let win_s = WindowId("win-s".to_owned());
    let win_t1 = WindowId("win-t1".to_owned());
    let win_t2 = WindowId("win-t2".to_owned());

    let mut layout = WorldLayout::new();
    layout.admission_seq_next = 5;
    let (source, counter) = layout
        .build_tiled_seed(&source_domain, 3, &[window("win-s", "out", "ws", 0, 100)])
        .expect("source seeds");
    assert_eq!(source.accepted_revision, 1);
    layout.admission_seq_next = counter;
    layout.domains.insert(source_key.clone(), source);

    let elsewhere = WindowId("elsewhere".to_owned());
    let observed = vec![
        window("win-t1", "out-2", "ws", 0, 400),
        window("win-t2", "out-2", "ws", 400, 400),
    ];
    let outcome = layout
        .reconcile_fresh(&target_domain, 3, &elsewhere, &observed)
        .expect("unique-source fresh adopt succeeds");

    assert_eq!(outcome.kind, FreshOutcomeKind::Legacy);
    assert_eq!(
        outcome.projection.base_revision, 1,
        "seed revision before anchor admit"
    );
    assert_eq!(outcome.projection.geometry.len(), 2);

    let kept = layout.domain(&source_key).expect("source preserved");
    assert_eq!(kept.accepted_revision, 1);
    assert!(kept.leaves.contains_key(&win_s));
    assert_eq!(kept.admission_order.get(&win_s), Some(&5));
    assert_eq!(kept.outer_gap, 3);

    let seeded = layout.domain(&target_key).expect("target seeded");
    assert!(seeded.leaves.contains_key(&win_t1));
    assert!(seeded.leaves.contains_key(&win_t2));
    assert_eq!(seeded.accepted_revision, 2);
    assert_eq!(seeded.outer_gap, 3);
    assert_eq!(layout.admission_seq_next, 8);
    assert!(layout.group_alloc.next_value() > 0);
}
