//! Domain/layout lifecycle tests (portable, headless) over `WorldLayout`.
//!
//! Ported from `tests/session_lifecycle.rs`:
//! - Preserved: admission tree structure (root leaf, binary wrap, nested
//!   axis, vertical tie), focus recency on removal, empty domain retention,
//!   recursive collapse, multi-domain/same-output isolation, refusal gates
//!   (duplicate, unknown, cross-domain, partial, malformed), exception
//!   deferral & removal, intentional float/unfloat geometry retention and
//!   centered fallback, deterministic replay, D2 fallback on tiny bounds,
//!   proportional shares, error redaction, policy version constants, and
//!   portable import hygiene.
//! - Retired (deleted `Session`/`Reconciler`/`Ack`/`Pending`/`Divergence`
//!   handshake): `stale_revision_diverges_terminal`,
//!   `invalid_capability_diverges`, `at_most_one_pending`,
//!   `owner_mismatch_diverges`,
//!   `dispatch_binds_identity_intent_capability_and_affected_ids`,
//!   `acknowledge_divergence_clears_pending_desired`,
//!   `verify_divergence_clears_pending_desired`, and the `Reconciler`
//!   tampered-plan check in `lifecycle_policy_version_bound_and_rejected_when_tampered`
//!   (the `POLICY_VERSION`/`LIFECYCLE_POLICY_VERSION` constant check is retained).

use std::collections::BTreeSet;

use crate::contract::{LIFECYCLE_POLICY_VERSION, POLICY_VERSION};

use crate::cosmic_v1::{proportional_insertion_shares, proportional_removal_shares};
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionBehavior, ExceptionFlags, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{AdmissionOutcome, LayoutFailure};
use super::state::{DomainLayout, WorldLayout};

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn placement(w: i32, h: i32) -> Rect {
    Rect { x: 0, y: 0, w, h }
}

fn tiled_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(window),
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

fn exception_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(window),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn domain_observed(
    layout: &WorldLayout,
    domain: &DomainKey,
    extra: Vec<ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(domain)
        .map(|w| tiled_observed(&w.0, &domain.output.0, &domain.workspace.0))
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut row = tiled_observed(&w.0, &domain.output.0, &domain.workspace.0);
        row.floating = flags.floating;
        row.fullscreen = flags.fullscreen;
        row.maximized = flags.maximized;
        row.sticky = flags.sticky;
        windows.push(row);
    }
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}

fn ensure_domain(layout: &mut WorldLayout, domain: &DomainKey, w: i32, h: i32) {
    if !layout.contains_domain(domain) {
        layout
            .domains
            .insert(domain.clone(), DomainLayout::new(placement(w, h), 0, 0));
    }
}

fn admit(
    layout: &mut WorldLayout,
    window: &str,
    output: &str,
    workspace: &str,
    bounds: Rect,
) -> AdmissionOutcome {
    let k = key(output, workspace);
    ensure_domain(layout, &k, 120, 80);
    let obs = domain_observed(layout, &k, vec![tiled_observed(window, output, workspace)]);
    layout
        .admit_window(
            &k,
            &win(window),
            ExceptionFlags::none(),
            None,
            bounds,
            false,
            false,
            &obs,
        )
        .unwrap_or_else(|e| panic!("admit {window} failed: {e:?}"))
}

fn close_window(layout: &mut WorldLayout, domain: &DomainKey, window: &str) {
    let obs: Vec<ObservedWindow> = domain_observed(layout, domain, Vec::new())
        .into_iter()
        .filter(|w| w.window.0 != window)
        .collect();
    layout
        .converge_domain(domain, &obs, None, &BTreeSet::new())
        .unwrap_or_else(|e| panic!("close {window} failed: {e:?}"));
}

fn leaves_of(layout: &WorldLayout, domain: &DomainKey) -> Vec<String> {
    match layout.tree(domain) {
        None => Vec::new(),
        Some(tree) => {
            let mut ids = Vec::new();
            collect(tree, &mut ids);
            ids
        }
    }
}

fn collect(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Leaf { id } => out.push(id.0.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect(child, out);
            }
        }
    }
}

fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn initial_admit_makes_root_leaf_focuses_and_projects() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    let outcome = admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1".to_string()]);
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-1".to_owned()))
    );
    assert_eq!(outcome.geometry.len(), 1);
    let rect = outcome.geometry[0].rect;
    assert!(rect.w > 0 && rect.h > 0);
    assert_eq!(rect, placement(120, 80));
    assert_eq!(layout.domain(&k).expect("dom").accepted_revision, 1);
}

#[test]
fn sibling_insert_after_focus_when_axes_match() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    match layout.tree(&k) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            assert_eq!(children[1].id().0, "leaf-win-2");
        }
        other => panic!("expected focused-root binary wrapper, got {other:?}"),
    }

    let outcome = admit(&mut layout, "win-3", "out-1", "ws-1", placement(120, 80));
    assert_eq!(
        leaves_of(&layout, &k),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    match layout.tree(&k) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected focused binary wrapper old/new, got {other:?}"),
            }
        }
        other => panic!("expected binary root wrapper, got {other:?}"),
    }
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(outcome.geometry.len(), 3);
    for i in 0..outcome.geometry.len() {
        let outer = placement(120, 80);
        let r = outcome.geometry[i].rect;
        assert!(r.w > 0 && r.h > 0);
        assert!(r.x >= outer.x && r.y >= outer.y);
        assert!(r.x + r.w <= outer.x + outer.w && r.y + r.h <= outer.y + outer.h);
        for j in (i + 1)..outcome.geometry.len() {
            assert!(!rects_intersect(&r, &outcome.geometry[j].rect), "overlap");
        }
    }
}

#[test]
fn no_focus_root_wraps_entire_root() {
    let mut layout = WorldLayout::new();
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-2");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-2", "ws-2", placement(120, 80));
    // Simulate focus having moved away from out-1/ws-1 so it has no eligible focus.
    layout.domains.get_mut(&k1).expect("k1").focused_leaf = None;

    admit(&mut layout, "win-3", "out-1", "ws-1", placement(120, 80));
    assert_eq!(
        leaves_of(&layout, &k1),
        vec!["leaf-win-1".to_string(), "leaf-win-3".to_string()]
    );
    match layout.tree(&k1) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            assert_eq!(children[1].id().0, "leaf-win-3");
        }
        other => panic!("expected no-focus root binary wrapper old/new, got {other:?}"),
    }
    assert_eq!(leaves_of(&layout, &k2), vec!["leaf-win-2".to_string()]);
    assert_eq!(
        layout.focus_leaf(&k1),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
}

#[test]
fn nesting_when_axes_differ_uses_input_orientation() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-3", "out-1", "ws-1", placement(80, 120));
    match layout.tree(&k) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected nested vertical group, got {other:?}"),
            }
        }
        other => panic!("expected root group, got {other:?}"),
    }
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
}

#[test]
fn vertical_selected_on_tie() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    ensure_domain(&mut layout, &k, 100, 100);
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(100, 100));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(100, 100));
    match layout.tree(&k) {
        Some(Node::Group { axis, .. }) => assert_eq!(*axis, Axis::Vertical),
        other => panic!("tie must select vertical, got {other:?}"),
    }
}

#[test]
fn focused_removal_uses_source_focus_recency_not_tree_sibling() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-3", "out-1", "ws-1", placement(120, 80));
    assert!(layout.sync_focus_from_window(&k, &win("win-1")));
    assert!(layout.sync_focus_from_window(&k, &win("win-2")));
    close_window(&mut layout, &k, "win-2");
    assert_eq!(
        leaves_of(&layout, &k),
        vec!["leaf-win-1".to_string(), "leaf-win-3".to_string()]
    );
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-1".to_owned()))
    );

    close_window(&mut layout, &k, "win-3");
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1".to_string()]);
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-1".to_owned()))
    );
}

#[test]
fn removal_prunes_unfocused_leaves_from_focus_recency() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-3", "out-1", "ws-1", placement(120, 80));
    close_window(&mut layout, &k, "win-1");
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
    close_window(&mut layout, &k, "win-3");
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
}

#[test]
fn removal_retains_empty_domains() {
    let mut layout = WorldLayout::new();
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-2");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(
        &mut layout,
        "win-other",
        "out-2",
        "ws-2",
        placement(120, 80),
    );
    close_window(&mut layout, &k1, "win-1");
    assert!(layout.contains_domain(&k1));
    assert!(layout.contains_domain(&k2));
    assert!(layout.tree(&k1).is_none());
    assert_eq!(layout.focus_leaf(&k1), None);
    assert_eq!(layout.tiled_count(&k2), 1);

    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    assert_eq!(leaves_of(&layout, &k1), vec!["leaf-win-2".to_string()]);
}

#[test]
fn nested_collapse_is_recursive() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-3", "out-1", "ws-1", placement(80, 120));
    close_window(&mut layout, &k, "win-3");
    assert_eq!(
        leaves_of(&layout, &k),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
    match layout.tree(&k) {
        Some(Node::Group { children, .. }) => assert_eq!(children.len(), 2),
        other => panic!("expected collapsed root group, got {other:?}"),
    }
}

#[test]
fn output_workspace_isolation() {
    let mut layout = WorldLayout::new();
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-2");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-2", "ws-2", placement(120, 80));
    assert_eq!(leaves_of(&layout, &k1), vec!["leaf-win-1".to_string()]);
    assert_eq!(leaves_of(&layout, &k2), vec!["leaf-win-2".to_string()]);
    close_window(&mut layout, &k1, "win-1");
    assert!(leaves_of(&layout, &k1).is_empty());
    assert_eq!(leaves_of(&layout, &k2), vec!["leaf-win-2".to_string()]);
    assert!(layout.contains_domain(&k1));
    assert!(layout.contains_domain(&k2));
}

#[test]
fn duplicate_unknown_crossdomain_partial_malformed_refused_without_corruption() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));

    // Duplicate.
    let obs = domain_observed(&layout, &k, Vec::new());
    assert_eq!(
        layout.admit_window(
            &k,
            &win("win-1"),
            ExceptionFlags::none(),
            None,
            placement(120, 80),
            false,
            false,
            &obs,
        ),
        Err(LayoutFailure::Semantics(RefusalKind::DuplicateWindow))
    );

    // Unknown window on toggle_orientation.
    assert_eq!(
        layout.toggle_orientation(&k, &win("nope"), &win("win-1"), &obs),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );

    // Cross-domain: unknown output.
    let obs_out9 = domain_observed(&layout, &k, vec![tiled_observed("win-2", "out-9", "ws-9")]);
    assert_eq!(
        layout.admit_window(
            &key("out-9", "ws-9"),
            &win("win-2"),
            ExceptionFlags::none(),
            None,
            placement(120, 80),
            false,
            false,
            &obs_out9,
        ),
        Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch))
    );

    // Cross-domain: workspace mismatch.
    let obs_ws9 = domain_observed(&layout, &k, vec![tiled_observed("win-2", "out-1", "ws-9")]);
    assert_eq!(
        layout.admit_window(
            &key("out-1", "ws-9"),
            &win("win-2"),
            ExceptionFlags::none(),
            None,
            placement(120, 80),
            false,
            false,
            &obs_ws9,
        ),
        Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch))
    );

    // Partial: omit known win-1 from observed while admitting win-2.
    let partial = vec![tiled_observed("win-2", "out-1", "ws-1")];
    assert_eq!(
        layout.admit_window(
            &k,
            &win("win-2"),
            ExceptionFlags::none(),
            None,
            placement(120, 80),
            false,
            false,
            &partial,
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );

    // Malformed: empty window id.
    assert_eq!(
        layout.toggle_float(&k, &win(""), None, &obs),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );

    // Malformed placement bounds.
    let obs2 = domain_observed(&layout, &k, vec![tiled_observed("win-2", "out-1", "ws-1")]);
    assert_eq!(
        layout.admit_window(
            &k,
            &win("win-2"),
            ExceptionFlags::none(),
            None,
            placement(0, 80),
            false,
            false,
            &obs2,
        ),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );

    // Still usable after refusals.
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    assert_eq!(
        leaves_of(&layout, &k),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
}

#[test]
fn exception_flags_fail_closed_until_behavior_selected() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    ensure_domain(&mut layout, &k, 120, 80);
    let obs = vec![exception_observed("win-9", "out-1", "ws-1")];
    let flags = ExceptionFlags {
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
    };
    assert_eq!(
        layout.admit_window(
            &k,
            &win("win-9"),
            flags,
            None,
            placement(120, 80),
            false,
            false,
            &obs,
        ),
        Err(LayoutFailure::Semantics(
            RefusalKind::ExceptionBehaviorUnselected
        ))
    );

    let outcome = layout
        .admit_window(
            &k,
            &win("win-9"),
            flags,
            Some(ExceptionBehavior::Defer),
            placement(120, 80),
            false,
            false,
            &obs,
        )
        .expect("deferred admits");
    assert!(outcome.geometry.is_empty());
    assert!(outcome.admitted_leaf.is_none());
    assert!(layout.is_exception(&k, &win("win-9")));
    assert!(leaves_of(&layout, &k).is_empty());

    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    assert_eq!(layout.exception_count(&k), 1);
}

#[test]
fn intentional_float_is_transactional_non_tree_state_and_unfloats_by_fresh_admission() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    let float_rect = Rect {
        x: 24,
        y: 16,
        w: 72,
        h: 48,
    };
    let obs = domain_observed(&layout, &k, Vec::new());
    let float = layout
        .toggle_float(&k, &win("win-1"), Some(float_rect), &obs)
        .expect("tiled window floats");
    assert!(float.geometry.is_empty());
    assert_eq!(layout.tiled_count(&k), 0);
    assert!(layout.is_exception(&k, &win("win-1")));
    assert!(
        layout
            .exception_flags(&k, &win("win-1"))
            .expect("flags")
            .floating
    );
    assert_eq!(
        layout.exception_float_geometry(&k, &win("win-1")),
        Some(float_rect)
    );

    let obs = domain_observed(&layout, &k, Vec::new());
    let tiled = layout
        .toggle_float(&k, &win("win-1"), None, &obs)
        .expect("floating window is freshly admitted");
    assert_eq!(tiled.geometry.len(), 1);
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1"]);
    assert_eq!(layout.exception_count(&k), 0);
    assert_eq!(
        layout.retained_float_geometry(&k, &win("win-1")),
        Some(float_rect)
    );
}

#[test]
fn intentional_float_retains_moved_geometry_across_float_unfloat_float() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    let first = Rect {
        x: 24,
        y: 16,
        w: 72,
        h: 48,
    };
    let obs = domain_observed(&layout, &k, Vec::new());
    layout
        .toggle_float(&k, &win("win-1"), Some(first), &obs)
        .expect("float 1");
    assert_eq!(
        layout.exception_float_geometry(&k, &win("win-1")),
        Some(first)
    );

    let moved = Rect {
        x: 30,
        y: 20,
        w: 50,
        h: 40,
    };
    let obs = domain_observed(&layout, &k, Vec::new());
    layout
        .toggle_float(&k, &win("win-1"), Some(moved), &obs)
        .expect("unfloat 1");
    assert_eq!(layout.exception_count(&k), 0);
    assert_eq!(
        layout.retained_float_geometry(&k, &win("win-1")),
        Some(moved)
    );

    let obs = domain_observed(&layout, &k, Vec::new());
    layout
        .toggle_float(&k, &win("win-1"), None, &obs)
        .expect("float 2");
    assert_eq!(
        layout.exception_float_geometry(&k, &win("win-1")),
        Some(moved)
    );
    assert_eq!(
        layout.retained_float_geometry(&k, &win("win-1")),
        Some(moved)
    );

    let obs = domain_observed(&layout, &k, Vec::new());
    let unfloated = layout
        .toggle_float(&k, &win("win-1"), Some(moved), &obs)
        .expect("unfloat 2");
    assert_eq!(unfloated.geometry.len(), 1);
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1"]);
    assert_eq!(layout.exception_count(&k), 0);
}

#[test]
fn intentional_float_centered_fallback_matches_adapter_placement() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    let obs = domain_observed(&layout, &k, Vec::new());
    layout
        .toggle_float(&k, &win("win-1"), None, &obs)
        .expect("center float");
    assert_eq!(
        layout.exception_float_geometry(&k, &win("win-1")),
        Some(Rect {
            x: 24,
            y: 16,
            w: 72,
            h: 48
        })
    );
}

fn assert_nested_h_v(layout: &WorldLayout, k: &DomainKey) {
    assert_eq!(
        leaves_of(layout, k),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    match layout.tree(k) {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group { axis, children, .. } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected inner V[win-2 win-3], got {other:?}"),
            }
        }
        other => panic!("expected root H[win-1 V[win-2 win-3]], got {other:?}"),
    }
}

#[test]
fn unfloat_reuses_admission_axis_not_domain_bounds() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-3", "out-1", "ws-1", placement(80, 120));
    assert_nested_h_v(&layout, &k);

    let obs = domain_observed(&layout, &k, Vec::new());
    layout
        .toggle_float(&k, &win("win-3"), None, &obs)
        .expect("float");
    assert_eq!(layout.exception_count(&k), 1);
    assert_eq!(
        leaves_of(&layout, &k),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
    let retained = layout
        .retained_float_geometry(&k, &win("win-3"))
        .expect("float retains placement");

    let obs = domain_observed(&layout, &k, Vec::new());
    layout
        .toggle_float(&k, &win("win-3"), None, &obs)
        .expect("unfloat");
    assert_eq!(layout.exception_count(&k), 0);
    assert_nested_h_v(&layout, &k);
    assert_eq!(
        layout.retained_float_geometry(&k, &win("win-3")),
        Some(retained)
    );
    assert_eq!(
        layout.focus_leaf(&k),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
}

#[test]
fn deterministic_replay() {
    fn run() -> (WorldLayout, Vec<AdmissionOutcome>) {
        let mut layout = WorldLayout::new();
        let outcomes = vec![
            admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80)),
            admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80)),
            admit(&mut layout, "win-3", "out-1", "ws-1", placement(80, 120)),
        ];
        (layout, outcomes)
    }
    let k = key("out-1", "ws-1");
    let (a_layout, a_outcomes) = run();
    let (b_layout, b_outcomes) = run();
    assert_eq!(a_layout.tree(&k), b_layout.tree(&k));
    assert_eq!(a_layout.focus_leaf(&k), b_layout.focus_leaf(&k));
    assert_eq!(a_outcomes, b_outcomes);
}

#[test]
fn tiny_bounds_projection_fails_closed() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    ensure_domain(&mut layout, &k, 2, 10);
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(10, 2));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(10, 2));
    let outcome = admit(&mut layout, "win-3", "out-1", "ws-1", placement(10, 2));
    assert_eq!(outcome.geometry.len(), 3);
    assert!(
        outcome
            .geometry
            .iter()
            .all(|g| g.rect.w > 0 && g.rect.h > 0),
        "every tile stays positive"
    );
}

#[test]
fn errors_are_redacted_and_bounded() {
    assert_eq!(RefusalKind::DuplicateWindow.as_str(), "duplicate-window");
    assert!(!RefusalKind::DuplicateWindow.message().contains("win-1"));
    assert!(!format!("{:?}", RefusalKind::CrossDomainMismatch).contains("SECRET"));
}

#[test]
fn policy_version_constants_stay_pinned() {
    assert_eq!(LIFECYCLE_POLICY_VERSION, 1);
    assert_eq!(POLICY_VERSION, 1);
}

#[test]
fn two_workspaces_on_same_output_are_isolated() {
    let mut layout = WorldLayout::new();
    let ka = key("out-1", "ws-a");
    let kb = key("out-1", "ws-b");
    ensure_domain(&mut layout, &ka, 120, 80);
    ensure_domain(&mut layout, &kb, 120, 80);

    admit(&mut layout, "win-a", "out-1", "ws-a", placement(120, 80));
    let outcome_b = admit(&mut layout, "win-b", "out-1", "ws-b", placement(120, 80));
    assert_eq!(leaves_of(&layout, &ka), vec!["leaf-win-a".to_string()]);
    assert_eq!(leaves_of(&layout, &kb), vec!["leaf-win-b".to_string()]);
    assert_eq!(
        layout.focus_leaf(&kb),
        Some(&NodeId("leaf-win-b".to_owned()))
    );
    assert_eq!(outcome_b.geometry.len(), 1);
    assert_eq!(outcome_b.geometry[0].workspace.0, "ws-b");
    assert_eq!(outcome_b.geometry[0].output.0, "out-1");

    close_window(&mut layout, &ka, "win-a");
    assert!(leaves_of(&layout, &ka).is_empty());
    assert_eq!(leaves_of(&layout, &kb), vec!["leaf-win-b".to_string()]);
    assert_eq!(
        layout.focus_leaf(&kb),
        Some(&NodeId("leaf-win-b".to_owned()))
    );

    close_window(&mut layout, &kb, "win-b");
    assert!(leaves_of(&layout, &ka).is_empty());
    assert!(leaves_of(&layout, &kb).is_empty());
    assert_eq!(layout.focus_leaf(&kb), None);
}

#[test]
fn deferred_removal_commits_without_topology_effect() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    ensure_domain(&mut layout, &k, 120, 80);
    let obs = vec![exception_observed("win-9", "out-1", "ws-1")];
    let flags = ExceptionFlags {
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
    };
    layout
        .admit_window(
            &k,
            &win("win-9"),
            flags,
            Some(ExceptionBehavior::Defer),
            placement(120, 80),
            false,
            false,
            &obs,
        )
        .expect("deferred admits");
    assert!(layout.is_exception(&k, &win("win-9")));
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1".to_string()]);
    let focus_before = layout.focus_leaf(&k).cloned();

    close_window(&mut layout, &k, "win-9");
    assert!(!layout.is_exception(&k, &win("win-9")));
    assert_eq!(layout.exception_count(&k), 0);
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1".to_string()]);
    assert_eq!(layout.focus_leaf(&k).cloned(), focus_before);
}

#[test]
fn inconsistent_exception_behavior_is_malformed() {
    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    ensure_domain(&mut layout, &k, 120, 80);
    let obs = vec![tiled_observed("win-1", "out-1", "ws-1")];
    assert_eq!(
        layout.admit_window(
            &k,
            &win("win-1"),
            ExceptionFlags::none(),
            Some(ExceptionBehavior::Defer),
            placement(120, 80),
            false,
            false,
            &obs,
        ),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    assert_eq!(leaves_of(&layout, &k), vec!["leaf-win-1".to_string()]);
}

#[test]
fn portable_session_module_prohibits_platform_imports() {
    const SESSION: &str = include_str!("../session.rs");
    const BANNED: &[&str] = &[
        "zbus",
        "rustix",
        "dbus",
        "tokio",
        "async-std",
        "smol",
        "wayland",
        "x11",
        "kwin",
        "libc",
        "nix",
        "socket",
        "ipc",
        "process",
        "platform",
        "std::process",
        "std::net",
        "std::os",
        "std::fs",
        "std::env",
        "std::thread",
        "ffi",
    ];
    for raw_line in SESSION.lines() {
        let line = raw_line.trim().to_ascii_lowercase();
        if !(line.starts_with("use ")
            || line.starts_with("extern crate")
            || line.starts_with("mod "))
        {
            continue;
        }
        for token in BANNED {
            assert!(
                !line.contains(token),
                "session.rs import line {raw_line:?} contains banned {token:?}"
            );
        }
    }
    assert!(
        SESSION.contains("Portable layout domain"),
        "session.rs should document its portable boundary"
    );
}

#[test]
fn uneven_proportional_insertion_and_removal_preserve_ratios() {
    let inserted = proportional_insertion_shares(&[389, 409], 2).expect("insert");
    assert_eq!(inserted, vec![389, 409, 399]);
    assert_eq!(inserted[0] * 409, inserted[1] * 389);
    let removed = proportional_removal_shares(&inserted, 2).expect("remove");
    assert_eq!(removed, vec![389, 409]);

    let mut layout = WorldLayout::new();
    let k = key("out-1", "ws-1");
    admit(&mut layout, "win-1", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-2", "out-1", "ws-1", placement(120, 80));
    admit(&mut layout, "win-3", "out-1", "ws-1", placement(120, 80));
    let tree = layout.tree(&k).cloned().expect("tree");
    match tree {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            assert_eq!(axis, Axis::Horizontal);
            assert_eq!(shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-win-1");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-win-2");
                    assert_eq!(children[1].id().0, "leaf-win-3");
                }
                other => panic!("expected focused binary wrapper, got {other:?}"),
            }
        }
        other => panic!("expected group, got {other:?}"),
    }
}
