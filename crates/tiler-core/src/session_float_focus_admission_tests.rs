#![forbid(unsafe_code)]
//! R-INS-05 float-focus admission + R-CLOSE-02 fresh-reopen fixtures (0.1 triage D05/D07).
//!
//! Verification-only characterization of the shared layout route in a
//! 120x80 test domain (gap 0):
//! - float-focus convergence: `converge_domain` with `focused_window = F`
//!   (ordinary float exception) admits C at the retained tiled focus B as
//!   nested `V[B,C]` (`H[A,V[B,C]]`; A keeps its 60x80 half, B/C take 60x40
//!   each); the float is preserved and the retained focus stays B until
//!   native observation follows C.
//! - genuine no-anchor admission still root-wraps when no tiled focus history
//!   exists at all.
//! - R-CLOSE-02: close focused B, explicitly refocus C, reopen fresh at C
//!   (`H[A,V[C,D]]`, newcomer focus), never an old-slot restore; survivor
//!   ratio rescale is not asserted here.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ObservedWindow, OutputDomain};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::{AdmissionOutcome, WorldLayout};

fn domain(output: &str, workspace: &str, w: i32, h: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn domain_key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn leaf_of(layout: &WorldLayout, key: &DomainKey, win: &str) -> NodeId {
    layout
        .leaves(key)
        .and_then(|leaves| leaves.get(&WindowId(win.to_owned())))
        .expect("leaf for window")
        .clone()
}

fn tiled_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
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

fn float_observed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
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

/// Complete observed window list for the domain plus optional extra entries.
fn complete_observed(
    layout: &WorldLayout,
    key: &DomainKey,
    extra: Vec<ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(key)
        .map(|w| tiled_observed(&w.0, &key.output.0, &key.workspace.0))
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(key) {
        windows.push(ObservedWindow {
            window: w.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            floating: flags.floating,
            fullscreen: flags.fullscreen,
            maximized: flags.maximized,
            sticky: flags.sticky,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        });
    }
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}

fn admit_with_placement(
    layout: &mut WorldLayout,
    key: &DomainKey,
    d: &OutputDomain,
    window: &str,
    placement_bounds: Rect,
) -> AdmissionOutcome {
    let win_obs = tiled_observed(window, &key.output.0, &key.workspace.0);
    if !layout.contains_domain(key) {
        let win_id = WindowId(window.to_owned());
        layout
            .converge_fresh_domain(d, 0, &[win_obs], Some(&win_id))
            .expect("fresh domain seed");
        return AdmissionOutcome {
            base_revision: 0,
            revision: layout.accepted_revision(key),
            admitted_leaf: layout.focus_leaf(key).cloned(),
            geometry: layout
                .project_retained_tiled_geometry(key, d.bounds, d.gap, &BTreeMap::new(), &[])
                .unwrap_or_default(),
        };
    }
    let obs = complete_observed(layout, key, vec![win_obs]);
    layout
        .admit_window(
            key,
            &WindowId(window.to_owned()),
            placement_bounds,
            false,
            false,
            &obs,
        )
        .unwrap_or_else(|e| panic!("admit {window} fails: {e:?}"))
}

fn remove_window(layout: &mut WorldLayout, key: &DomainKey, window: &str) {
    let target = WindowId(window.to_owned());
    let obs: Vec<ObservedWindow> = complete_observed(layout, key, Vec::new())
        .into_iter()
        .filter(|entry| entry.window != target)
        .collect();
    let mut closed = BTreeSet::new();
    closed.insert(target);
    layout
        .converge_domain(key, &obs, None, &closed)
        .unwrap_or_else(|e| panic!("remove {window} fails: {e:?}"));
}

#[test]
fn float_focus_convergence_admits_newcomer_at_prior_tiled_focus() {
    // R-INS-05 retained-route characterization on the unchanged policy: the
    // native signal is `focused_window = F` (ordinary float exception)
    // through the actual convergence primitive. Float focus never
    // takes tile focus, so C tiles at the retained tiled focus B under the
    // ordinary long-edge rule (B is 60x80 tall, so Vertical): `H[A,V[B,C]]`
    // with A keeping its half. The retained focus stays B, and F is
    // preserved. A later observation with native focus on C syncs focus to
    // the newcomer.
    let mut layout = WorldLayout::new();
    let d = domain("out-1", "ws-1", 120, 80);
    let key = domain_key("out-1", "ws-1");
    let wide = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    admit_with_placement(&mut layout, &key, &d, "win-a", wide);
    admit_with_placement(&mut layout, &key, &d, "win-b", wide);

    // Float F appears with native focus on it.
    let obs_f = complete_observed(
        &layout,
        &key,
        vec![float_observed("win-f", "out-1", "ws-1")],
    );
    let counts = layout
        .converge_domain(
            &key,
            &obs_f,
            Some(&WindowId("win-f".to_owned())),
            &BTreeSet::new(),
        )
        .expect("float converges");
    assert_eq!(counts.flags_adopted, 1);
    assert!(layout.is_exception(&key, &WindowId("win-f".to_owned())));
    // Prior valid tiled focus B is retained: a float never takes leaf focus.
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "win-b"))
    );

    // Newcomer C opens while F holds native focus.
    let obs_c = complete_observed(
        &layout,
        &key,
        vec![tiled_observed("win-c", "out-1", "ws-1")],
    );
    let counts = layout
        .converge_domain(
            &key,
            &obs_c,
            Some(&WindowId("win-f".to_owned())),
            &BTreeSet::new(),
        )
        .expect("newcomer converges");
    assert_eq!(counts.admitted, 1);
    match layout.tree(&key) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(
                *axis,
                Axis::Horizontal,
                "root stays H so A keeps its left half"
            );
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id(), &leaf_of(&layout, &key, "win-a"));
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(
                        *axis,
                        Axis::Vertical,
                        "tall B (60x80) splits Vertical under long-edge admission"
                    );
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id(), &leaf_of(&layout, &key, "win-b"));
                    assert_eq!(children[1].id(), &leaf_of(&layout, &key, "win-c"));
                }
                other => panic!("expected nested V[B,C] beside A, got {other:?}"),
            }
        }
        other => panic!("expected H[A,V[B,C]], got {other:?}"),
    }
    // Concrete projected frames for the 120x80 test domain (gap 0) via the
    // shared retained-geometry read path: A keeps its 60x80 left half, B/C
    // split the right half 60x40 each.
    let mut frames: Vec<(String, Rect)> = layout
        .project_retained_tiled_geometry(&key, d.bounds, d.gap, &BTreeMap::new(), &[])
        .expect("projects")
        .into_iter()
        .map(|g| (g.window.0, g.rect))
        .collect();
    frames.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        frames,
        vec![
            (
                "win-a".to_owned(),
                Rect {
                    x: 0,
                    y: 0,
                    w: 60,
                    h: 80
                }
            ),
            (
                "win-b".to_owned(),
                Rect {
                    x: 60,
                    y: 0,
                    w: 60,
                    h: 40
                }
            ),
            (
                "win-c".to_owned(),
                Rect {
                    x: 60,
                    y: 40,
                    w: 60,
                    h: 40
                }
            ),
        ],
        "A keeps its half; B/C stack vertically in the right half"
    );
    assert!(layout.is_exception(&key, &WindowId("win-f".to_owned())));
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "win-b")),
        "float focus never displaces the retained tiled focus"
    );

    // Native focus follows the newcomer: focus syncs to C with no topology change.
    let obs_follow = complete_observed(&layout, &key, Vec::new());
    let counts = layout
        .converge_domain(
            &key,
            &obs_follow,
            Some(&WindowId("win-c".to_owned())),
            &BTreeSet::new(),
        )
        .expect("focus follow converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "win-c"))
    );
}

#[test]
fn seeded_mru_admission_anchors_without_focus() {
    // No-focus boundary characterization under cursor-first placement: a
    // multi-leaf tree built with no tiled focus history at all (focusless
    // converges, so no evolving focus anywhere) anchors the next admission
    // at the seeded MRU leaf — the per-leg commit history the D8 seeding
    // preserves for focused-removal fallback — rather than root-wrapping.
    // Genuine per-domain focus history (remembered last-active) would win
    // over this seed; with none anywhere the seed is the last activity.
    let mut layout = WorldLayout::new();
    let d = domain("out-1", "ws-1", 120, 80);
    let key = domain_key("out-1", "ws-1");
    let win_a = tiled_observed("win-a", "out-1", "ws-1");
    let counts = layout
        .converge_fresh_domain(&d, 0, &[win_a], None)
        .expect("converge win-a");
    assert_eq!(counts.admitted, 1);
    assert_eq!(layout.focus_leaf(&key), None);

    let obs_b = complete_observed(
        &layout,
        &key,
        vec![tiled_observed("win-b", "out-1", "ws-1")],
    );
    let counts = layout
        .converge_domain(&key, &obs_b, None, &BTreeSet::new())
        .expect("converge win-b");
    assert_eq!(counts.admitted, 1);
    assert_eq!(layout.focus_leaf(&key), None);

    let obs_c = complete_observed(
        &layout,
        &key,
        vec![tiled_observed("win-c", "out-1", "ws-1")],
    );
    let counts = layout
        .converge_domain(&key, &obs_c, None, &BTreeSet::new())
        .expect("third converge admits");
    assert_eq!(counts.admitted, 1);
    match layout.tree(&key) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id(), &leaf_of(&layout, &key, "win-a"));
            match &children[1] {
                Node::Group { children, .. } => {
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id(), &leaf_of(&layout, &key, "win-b"));
                    assert_eq!(children[1].id(), &leaf_of(&layout, &key, "win-c"));
                }
                other => panic!("expected wrapped H[B,C] second, got {other:?}"),
            }
        }
        other => panic!("expected H[A,H[B,C]], got {other:?}"),
    }
    assert_eq!(layout.focus_leaf(&key), None);
}

#[test]
fn close_then_reopen_is_fresh_admission_at_refocus_not_old_slot() {
    // R-CLOSE-02 (D07): H[A,B,C] with B active; close focused B (survivors
    // keep order, MRU fallback would land on A here); explicitly refocus the
    // different tile C; reopen a same-app window. Fresh admission anchors at
    // C (`H[A,V[C,D]]`, newcomer focus), never an old-slot restore. Survivor
    // manual-ratio rescale is not asserted here.
    let mut layout = WorldLayout::new();
    let d = domain("out-1", "ws-1", 120, 80);
    let key = domain_key("out-1", "ws-1");
    let wide = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    admit_with_placement(&mut layout, &key, &d, "win-a", wide);
    admit_with_placement(&mut layout, &key, &d, "win-b", wide);
    admit_with_placement(&mut layout, &key, &d, "win-c", wide);
    // End state of the row Given: B active (stack tops B over A over C).
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-a".to_owned())));
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "win-b"))
    );

    // Close focused B: survivors keep order.
    remove_window(&mut layout, &key, "win-b");
    let leaves: Vec<String> = match layout.tree(&key) {
        Some(Node::Group { children, .. }) => children.iter().map(|c| c.id().0.clone()).collect(),
        other => panic!("expected surviving H[A,C], got {other:?}"),
    };
    assert_eq!(
        leaves,
        vec![
            leaf_of(&layout, &key, "win-a").0,
            leaf_of(&layout, &key, "win-c").0
        ]
    );

    // Explicitly refocus the different tile C (the row's "focus C" step).
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-c".to_owned())));
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "win-c"))
    );

    // Reopen a same-app window under a new id: fresh admission at C.
    let outcome = admit_with_placement(
        &mut layout,
        &key,
        &d,
        "win-b2",
        Rect {
            x: 60,
            y: 0,
            w: 60,
            h: 80,
        },
    );
    match layout.tree(&key) {
        Some(Node::Group { axis, children, .. }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id(), &leaf_of(&layout, &key, "win-a"));
            match &children[1] {
                Node::Group { axis, children, .. } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(children.len(), 2);
                    // Fresh at focused C: C first, newcomer after. An old-slot
                    // restore would order the reopened window before C.
                    assert_eq!(children[0].id(), &leaf_of(&layout, &key, "win-c"));
                    assert_eq!(children[1].id(), &leaf_of(&layout, &key, "win-b2"));
                }
                other => panic!("expected fresh V[C,D] beside A, got {other:?}"),
            }
        }
        other => panic!("expected H[A,V[C,D]], got {other:?}"),
    }
    assert_eq!(
        layout.focus_leaf(&key),
        Some(&leaf_of(&layout, &key, "win-b2"))
    );
    assert_eq!(outcome.geometry.len(), 3);
}

/// First layout leaf window for one domain, in deterministic child order
/// (tentative no-history tie-break for view returns).
fn first_leaf_window(layout: &WorldLayout, key: &DomainKey) -> Option<WindowId> {
    let mut node = layout.tree(key)?;
    loop {
        match node {
            Node::Leaf { id } => return layout.window_for_leaf(key, id),
            Node::Group { children, .. } => {
                node = children.first()?;
            }
        }
    }
}

#[test]
fn view_return_candidates_prefers_remembered_then_mru_then_first() {
    // Shared-core view-return chain: a fresh converge with no observed
    // focus records no remembered leaf while the MRU stack seeds in
    // admission order, so the first candidate is the MRU top (win-b), not
    // the first layout leaf (win-a). A remembered leaf wins over a newer
    // MRU top, ineligible top candidates fall through per candidate at
    // the call site, and an empty domain resolves to no candidate.
    fn first_candidate(layout: &WorldLayout, key: &DomainKey) -> Option<NodeId> {
        layout.view_return_candidates(key).into_iter().next()
    }
    let mut layout = WorldLayout::new();
    let d = domain("out-1", "ws-1", 120, 80);
    let key = domain_key("out-1", "ws-1");
    layout
        .converge_fresh_domain(
            &d,
            0,
            &[
                tiled_observed("win-a", "out-1", "ws-1"),
                tiled_observed("win-b", "out-1", "ws-1"),
            ],
            None,
        )
        .expect("converge");
    assert_eq!(layout.focus_leaf(&key), None);
    assert_eq!(
        first_leaf_window(&layout, &key)
            .as_ref()
            .map(|w| w.0.as_str()),
        Some("win-a")
    );
    assert_eq!(
        first_candidate(&layout, &key),
        Some(leaf_of(&layout, &key, "win-b")),
        "MRU top wins with no remembered leaf"
    );
    // Ordinary activation advances the MRU stack only; the remembered
    // leaf still wins the return.
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    assert!(layout.remember_focus(&key, &WindowId("win-a".to_owned())));
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    assert_eq!(
        first_candidate(&layout, &key),
        Some(leaf_of(&layout, &key, "win-a")),
        "remembered leaf beats the newer MRU top"
    );
    // Candidate order documents the fall-through: remembered first, then
    // MRU recency, then layout order, deduplicated.
    assert_eq!(
        layout.view_return_candidates(&key),
        vec![
            leaf_of(&layout, &key, "win-a"),
            leaf_of(&layout, &key, "win-b")
        ]
    );
    // Empty domain: no candidate, never invented focus.
    let empty = WorldLayout::new();
    assert_eq!(first_candidate(&empty, &key), None);
}
