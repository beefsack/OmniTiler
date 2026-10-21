#![forbid(unsafe_code)]
//! AR12 size-hint integration tests (portable, headless).
//!
//! Drives the real `Session` admission path plus the pure reconcile
//! projection ([`crate::boundary::project_retained_tiled_geometry`]):
//! satisfiable minimums take sibling slack with shares untouched,
//! unsatisfiable minimums keep the proportional fallback flagged
//! `overconstrained` (projection never refuses for minimums alone), short
//! frames at a real carried maximum accept as `client_clamped` without
//! perturbing shares or staging a plan, and unhinted, position, or
//! increment-scale drift never accepts.

use std::collections::BTreeMap;

use crate::boundary::{ProjectionKind, project_retained_tiled_geometry};
use crate::contract::{LifecycleCapabilities, LifecyclePostObservation, Observation};
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
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

fn observed_window(window: &str, hints: WindowSizeHints) -> ObservedWindow {
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
        hints,
    }
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

/// Committed two-window session: root `H[a( win-a ), b( win-b )]` shares
/// `[1, 1]`, so each desired rectangle is 600x800 before hints.
fn committed_pair() -> (Session, DomainKey) {
    let owner = owner();
    let generation = generation();
    let mut session =
        Session::new(owner.clone(), generation.clone(), 0, 0, vec![domain()]).expect("session");
    let key = session.domains().first().expect("domain").key();
    let tree = Node::Group {
        id: NodeId::from("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId::from("a"),
            },
            Node::Leaf {
                id: NodeId::from("b"),
            },
        ],
        shares: vec![1, 1],
    };
    let links = vec![
        crate::directional::WindowLink {
            window: WindowId("win-a".to_owned()),
            leaf: NodeId::from("a"),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
        },
        crate::directional::WindowLink {
            window: WindowId("win-b".to_owned()),
            leaf: NodeId::from("b"),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
        },
    ];
    let observation = SessionObservation {
        observation: Observation::new(owner.clone(), generation.clone(), 0, 0),
        windows: vec![
            observed_window("win-a", hints_none()),
            observed_window("win-b", hints_none()),
        ],
    };
    let correlation = CorrelationId::parse("size-hint-seed").expect("valid");
    let plan = session
        .propose_fitted_admit(
            tree,
            links,
            NodeId::from("a"),
            &WindowId("win-a".to_owned()),
            &key.output,
            &key.workspace,
            &observation,
            &correlation,
            &LifecycleCapabilities::full(),
        )
        .expect("fit");
    let base = plan.dispatch.base_revision;
    let ack = crate::contract::AdapterAck::new(
        correlation.clone(),
        owner.clone(),
        generation.clone(),
        base,
        crate::contract::AckOutcome::Accepted,
    );
    session.acknowledge(&ack).expect("ack");
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner, generation, base, 0),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
    (session, key)
}

fn project(
    session: &Session,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    observed: &[EngineWindow],
) -> Vec<crate::session::DesiredGeometry> {
    let retained = session.domains().first().expect("domain").clone();
    let (focus_domain, focus_leaf) = session.focus();
    project_retained_tiled_geometry(
        session,
        key,
        retained.bounds,
        retained.gap,
        Some((
            focus_domain.expect("focus domain"),
            focus_leaf.expect("focus leaf"),
        )),
        ProjectionKind::Reconcile,
        hints,
        observed,
    )
    .expect("projects")
    .geometry
}

fn rect_by_window(geometry: &[crate::session::DesiredGeometry]) -> BTreeMap<String, Rect> {
    geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.rect))
        .collect()
}

fn shares_of(session: &Session, key: &DomainKey) -> Vec<u64> {
    match session
        .snapshot()
        .domains
        .into_iter()
        .find(|d| d.output == key.output && d.workspace == key.workspace)
        .and_then(|d| d.tree)
        .expect("tree")
    {
        Node::Group { shares, .. } => shares,
        Node::Leaf { .. } => panic!("expected root group"),
    }
}

#[test]
fn satisfiable_minimum_takes_sibling_slack_with_shares_untouched() {
    let (session, key) = committed_pair();
    let mut hints = BTreeMap::new();
    hints.insert(
        WindowId("win-b".to_owned()),
        WindowSizeHints {
            min_w: Some(700),
            ..hints_none()
        },
    );
    let geometry = project(&session, &key, &hints, &[]);
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
    assert_eq!(shares_of(&session, &key), vec![1, 1]);
}

#[test]
fn overconstrained_minimums_keep_proportional_fallback_and_flag() {
    let (session, key) = committed_pair();
    let before_snapshot = session.snapshot();
    let before_revision = session.accepted_revision();
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
    let geometry = project(&session, &key, &hints, &[]);
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
    // Accepted clamp never perturbs retained state nor synthesizes a plan:
    // the pure projection leaves the session exactly alone.
    assert_eq!(session.snapshot(), before_snapshot);
    assert_eq!(session.accepted_revision(), before_revision);
    assert!(!session.has_pending());
    assert_eq!(shares_of(&session, &key), vec![1, 1]);
}

#[test]
fn ghostty_like_short_frame_accepts_client_clamp_without_reassert() {
    let (session, key) = committed_pair();
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
    let geometry = project(&session, &key, &hints, &observed);
    let by_window: BTreeMap<String, &crate::session::DesiredGeometry> =
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
    assert_eq!(shares_of(&session, &key), vec![1, 1]);
    assert!(!session.has_pending());
}

#[test]
fn unhinted_drift_never_accepts_and_still_fully_reasserts() {
    let (session, key) = committed_pair();
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
    let geometry = project(&session, &key, &BTreeMap::new(), &observed);
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
    let (session, key) = committed_pair();
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
    let geometry = project(&session, &key, &hints, &observed);
    assert!(
        geometry.iter().all(|g| !g.client_clamped),
        "position drift never accepts"
    );
}

#[test]
fn exception_windows_bypass_hint_and_clamp_paths() {
    let (session, key) = committed_pair();
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
    let geometry = project(&session, &key, &hints, &observed);
    // win-b's minimums are unsatisfiable, so it keeps the proportional
    // fallback flagged overconstrained; the fit-excluded observed entry is
    // skipped for clamp assessment (never adopted, never flagged).
    let rects = rect_by_window(&geometry);
    assert_eq!(rects["win-b"].w, 600);
    let by_window: BTreeMap<String, &crate::session::DesiredGeometry> =
        geometry.iter().map(|g| (g.window.0.clone(), g)).collect();
    assert!(by_window["win-b"].overconstrained);
    assert!(!by_window["win-a"].overconstrained);
    assert!(geometry.iter().all(|g| !g.client_clamped));
    assert_eq!(shares_of(&session, &key), vec![1, 1]);
}
