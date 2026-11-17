#![forbid(unsafe_code)]
//! AR12 size-hint integration tests (portable, headless).
//!
//! Drives [`WorldLayout::reconcile_fresh`] plus the retained reconcile
//! projection ([`WorldLayout::project_retained_tiled_geometry`]):
//! satisfiable minimums take sibling slack with shares untouched,
//! unsatisfiable minimums keep the proportional fallback flagged
//! `overconstrained` (projection never refuses for minimums alone), short
//! frames at a real carried maximum accept as `client_clamped` without
//! perturbing shares or mutating retained state, and unhinted, position, or
//! increment-scale drift never accepts.

use std::collections::BTreeMap;

use crate::directional::{Axis, Node, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DesiredGeometry, DomainKey, OutputDomain};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::WorldLayout;

fn domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn hints_none() -> WindowSizeHints {
    WindowSizeHints::none()
}

fn engine_window(window: &str, rect: Rect, hints: WindowSizeHints) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints,
    }
}

/// Committed two-window layout: root `H[a( win-a ), b( win-b )]` shares
/// `[1, 1]`, so each desired rectangle is 600x800 before hints.
fn committed_pair() -> (WorldLayout, DomainKey) {
    let d = domain();
    let key = d.key();
    let mut layout = WorldLayout::new();
    let observed = vec![
        engine_window(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 600,
                h: 800,
            },
            hints_none(),
        ),
        engine_window(
            "win-b",
            Rect {
                x: 600,
                y: 0,
                w: 600,
                h: 800,
            },
            hints_none(),
        ),
    ];
    layout
        .reconcile_fresh(&d, 0, &WindowId("win-a".to_owned()), &observed[..1])
        .expect("fit");
    layout
        .converge_retained_observation(&key, &observed, &WindowId("win-a".to_owned()), &[])
        .expect("converge win-b");
    layout
        .reconcile_retained(
            &key,
            d.bounds,
            d.gap,
            0,
            &WindowId("win-a".to_owned()),
            &observed,
            &[],
        )
        .expect("reconcile win-b");
    assert_eq!(
        match layout.tree(&key).expect("tree") {
            Node::Group { axis, shares, .. } => (*axis, shares.clone()),
            Node::Leaf { .. } => panic!("expected root group"),
        },
        (Axis::Horizontal, vec![1, 1])
    );
    (layout, key)
}

fn project(
    layout: &WorldLayout,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    observed: &[EngineWindow],
) -> Vec<DesiredGeometry> {
    let (bounds, gap) = layout.domain_geometry(key).expect("domain geometry");
    layout
        .project_retained_tiled_geometry(key, bounds, gap, hints, observed)
        .expect("projects")
}

fn rect_by_window(geometry: &[DesiredGeometry]) -> BTreeMap<String, Rect> {
    geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.rect))
        .collect()
}

fn shares_of(layout: &WorldLayout, key: &DomainKey) -> Vec<u64> {
    match layout.tree(key).expect("tree") {
        Node::Group { shares, .. } => shares.clone(),
        Node::Leaf { .. } => panic!("expected root group"),
    }
}

#[test]
fn satisfiable_minimum_takes_sibling_slack_with_shares_untouched() {
    let (layout, key) = committed_pair();
    let mut hints = BTreeMap::new();
    hints.insert(
        WindowId("win-b".to_owned()),
        WindowSizeHints {
            min_w: Some(700),
            ..hints_none()
        },
    );
    let geometry = project(&layout, &key, &hints, &[]);
    let rects = rect_by_window(&geometry);
    // win-b rises 600 -> 700; win-a yields 600 -> 500.
    assert_eq!(
        rects["win-b"],
        Rect {
            x: 500,
            y: 0,
            w: 700,
            h: 800
        },
        "minimum honored at the sibling's expense, position contiguous"
    );
    assert_eq!(rects["win-a"].w, 500);
    assert_eq!(rects["win-a"].x, 0);
    for entry in &geometry {
        assert!(!entry.overconstrained, "satisfiable honors flag nothing");
        assert!(!entry.client_clamped, "no observed drift assessed");
    }
    // Retained shares never move for hint honoring.
    assert_eq!(shares_of(&layout, &key), vec![1, 1]);
}

#[test]
fn overconstrained_minimums_keep_proportional_fallback_and_flag() {
    let (layout, key) = committed_pair();
    let before_tree = layout.tree(&key).cloned();
    let before_revision = layout.accepted_revision(&key);
    let mut hints = BTreeMap::new();
    for window in ["win-a", "win-b"] {
        hints.insert(
            WindowId(window.to_owned()),
            WindowSizeHints {
                min_w: Some(700),
                ..hints_none()
            },
        );
    }
    // 700 + 700 exceeds the 1200 extent: unsatisfiable.
    let geometry = project(&layout, &key, &hints, &[]);
    let rects = rect_by_window(&geometry);
    assert_eq!(rects["win-a"].w, 600, "fallback keeps proportional shares");
    assert_eq!(rects["win-b"].w, 600);
    for entry in &geometry {
        assert!(entry.overconstrained, "unsatisfiable windows flagged");
        assert!(
            !entry.client_clamped,
            "overconstrained needs no second flag"
        );
    }
    // Accepted clamp never perturbs retained state:
    // the pure projection leaves the layout exactly alone.
    assert_eq!(layout.tree(&key).cloned(), before_tree);
    assert_eq!(layout.accepted_revision(&key), before_revision);
    assert_eq!(shares_of(&layout, &key), vec![1, 1]);
}

#[test]
fn ghostty_like_short_frame_accepts_client_clamp_without_reassert() {
    let (layout, key) = committed_pair();
    // Desired win-b is 600x800 at (600, 0); the client reports a REAL
    // maximum of 744 high and renders exactly there (same position,
    // smaller size). Distinct from the logged Ghostty evidence (unbounded
    // max sentinel), which stays drift.
    let short_b = Rect {
        x: 600,
        y: 0,
        w: 600,
        h: 744,
    };
    let mut hints = BTreeMap::new();
    let win_b_hints = WindowSizeHints {
        max_h: Some(744),
        ..hints_none()
    };
    hints.insert(WindowId("win-b".to_owned()), win_b_hints);
    let observed = vec![
        engine_window(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 600,
                h: 800,
            },
            hints_none(),
        ),
        engine_window("win-b", short_b, win_b_hints),
    ];
    let geometry = project(&layout, &key, &hints, &observed);
    let by_window: BTreeMap<String, &DesiredGeometry> =
        geometry.iter().map(|g| (g.window.0.clone(), g)).collect();
    // Desired truth is retained (leftover explained, not rewritten)...
    assert_eq!(
        by_window["win-b"].rect,
        Rect {
            x: 600,
            y: 0,
            w: 600,
            h: 800
        }
    );
    // ...while the observed clamp accepts: no rewrite, no drift/park input.
    assert!(by_window["win-b"].client_clamped);
    assert!(!by_window["win-b"].overconstrained);
    assert!(!by_window["win-a"].client_clamped);
    assert!(!by_window["win-a"].overconstrained);
    assert_eq!(shares_of(&layout, &key), vec![1, 1]);
}

#[test]
fn unhinted_drift_never_accepts_and_still_fully_reasserts() {
    let (layout, key) = committed_pair();
    // Same short frame but no hints anywhere: drift, not a clamp.
    let observed = vec![
        engine_window(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 600,
                h: 800,
            },
            hints_none(),
        ),
        engine_window(
            "win-b",
            Rect {
                x: 600,
                y: 0,
                w: 600,
                h: 744,
            },
            hints_none(),
        ),
    ];
    let geometry = project(&layout, &key, &BTreeMap::new(), &observed);
    for entry in &geometry {
        assert!(!entry.client_clamped, "unhinted drift never adopts");
        assert!(!entry.overconstrained);
    }
    // Full retained allocation still returned for reassertion.
    assert_eq!(
        rect_by_window(&geometry)["win-b"],
        Rect {
            x: 600,
            y: 0,
            w: 600,
            h: 800
        }
    );
}

#[test]
fn position_drift_never_accepts_even_with_hints() {
    let (layout, key) = committed_pair();
    let mut hints = BTreeMap::new();
    let win_b_hints = WindowSizeHints {
        max_h: Some(744),
        ..hints_none()
    };
    hints.insert(WindowId("win-b".to_owned()), win_b_hints);
    // Shifted position with a clamped size: clients clamp sizes, not
    // positions, so this stays drift.
    let observed = vec![
        engine_window(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 600,
                h: 800,
            },
            hints_none(),
        ),
        engine_window(
            "win-b",
            Rect {
                x: 604,
                y: 0,
                w: 600,
                h: 744,
            },
            win_b_hints,
        ),
    ];
    let geometry = project(&layout, &key, &hints, &observed);
    assert!(
        geometry.iter().all(|g| !g.client_clamped),
        "position drift never accepts"
    );
}

#[test]
fn exception_windows_bypass_hint_and_clamp_paths() {
    let (layout, key) = committed_pair();
    // A fit-excluded (fullscreen/maximized-class) observed entry with a wild
    // rectangle plus aggressive hints must not assess, flag, or fail: such
    // rects are compositor-owned, never layout-driven.
    let mut excluded_b = engine_window(
        "win-b",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        WindowSizeHints {
            min_w: Some(1200),
            min_h: Some(800),
            max_w: None,
            max_h: None,
        },
    );
    excluded_b.fit_excluded = true;
    let observed = vec![
        engine_window(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 600,
                h: 800,
            },
            hints_none(),
        ),
        excluded_b,
    ];
    let mut hints = BTreeMap::new();
    hints.insert(
        WindowId("win-b".to_owned()),
        WindowSizeHints {
            min_w: Some(1200),
            min_h: Some(800),
            ..hints_none()
        },
    );
    let geometry = project(&layout, &key, &hints, &observed);
    // win-b's minimums are unsatisfiable, so it keeps the proportional
    // fallback flagged overconstrained; the fit-excluded observed entry is
    // skipped for clamp assessment (never adopted, never flagged).
    let rects = rect_by_window(&geometry);
    assert_eq!(rects["win-b"].w, 600);
    let by_window: BTreeMap<String, &DesiredGeometry> =
        geometry.iter().map(|g| (g.window.0.clone(), g)).collect();
    assert!(by_window["win-b"].overconstrained);
    assert!(!by_window["win-a"].overconstrained);
    assert!(geometry.iter().all(|g| !g.client_clamped));
    assert_eq!(shares_of(&layout, &key), vec![1, 1]);
}
