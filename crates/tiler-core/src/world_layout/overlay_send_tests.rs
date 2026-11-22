//! D01 native activation ownership (admission over an overlay) + D28
//! fullscreen workspace-send carry + CLOSE-05 overlay removal (portable,
//! headless).
//!
//! Drives the real `WorldLayout` convergence/send/remove primitives:
//! - D01: admitting C while B is fullscreen/maximized retains B's slot
//!   (leaf identity) and follows the observed native focus: host-denied
//!   (focus stays B) keeps B focused, host-approved (focus moves to C)
//!   focuses C.
//! - D28: an explicit send of a retained fullscreen tiled mover plans the
//!   ordinary `PairSendKind::Tiled` transfer with its leaf preserved and
//!   follow/stay focus intact (no unfullscreen in core).
//! - CLOSE-05: removing a fullscreen/maximized focused B refills survivor
//!   A and focuses A.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain};
use crate::size_hints::WindowSizeHints;

use super::outcome::PairSendKind;
use super::pair_operation::PairOperationContext;
use super::state::{DomainLayout, WorldLayout};

fn bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    }
}

fn dom(o: &str, w: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
        bounds: bounds(),
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn key(o: &str, w: &str) -> DomainKey {
    DomainKey {
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(w),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn flagged(base: &ObservedWindow, fullscreen: bool, maximized: bool) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = fullscreen;
    entry.maximized = maximized;
    entry
}

fn eng_from_observed(obs: &ObservedWindow) -> EngineWindow {
    EngineWindow {
        window: obs.window.clone(),
        output: obs.output.clone(),
        workspace: obs.workspace.clone(),
        rect: bounds(),
        floating: obs.floating,
        fit_excluded: false,
        fullscreen: obs.fullscreen,
        maximized: obs.maximized,
        sticky: obs.sticky,
        fixed_auto: obs.fixed_auto,
        fixed_suppress: obs.fixed_suppress,
        hints: obs.hints,
    }
}

fn obs_for(
    layout: &WorldLayout,
    domain: &DomainKey,
    extra: Vec<ObservedWindow>,
    overrides: &BTreeMap<String, ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut wins: Vec<ObservedWindow> = layout
        .tiled_windows(domain)
        .map(|w| {
            if let Some(entry) = overrides.get(&w.0) {
                entry.clone()
            } else {
                tiled(&w.0, &domain.output.0, &domain.workspace.0)
            }
        })
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut entry = tiled(&w.0, &domain.output.0, &domain.workspace.0);
        entry.floating = flags.floating;
        entry.fullscreen = flags.fullscreen;
        entry.maximized = flags.maximized;
        entry.sticky = flags.sticky;
        wins.push(entry);
    }
    wins.extend(extra);
    wins
}

fn admit(layout: &mut WorldLayout, w: &str, o: &str, ws: &str) {
    let k = key(o, ws);
    if !layout.contains_domain(&k) {
        layout
            .domains
            .insert(k.clone(), DomainLayout::new(bounds(), 0, 0));
    }
    let o0 = obs_for(layout, &k, vec![tiled(w, o, ws)], &BTreeMap::new());
    layout
        .admit_window(
            &k,
            &win(w),
            Rect {
                x: 0,
                y: 0,
                w: 60,
                h: 40,
            },
            false,
            false,
            &o0,
        )
        .expect("admit");
}

fn leaf_of(layout: &WorldLayout, domain: &DomainKey, window: &str) -> NodeId {
    layout
        .bindings(domain)
        .find_map(|(w, leaf)| (w.0 == window).then(|| leaf.clone()))
        .expect("link")
}

fn focused_window(layout: &WorldLayout, domain: &DomainKey) -> WindowId {
    let leaf = layout.focus_leaf(domain).expect("focused leaf");
    layout.window_for_leaf(domain, leaf).expect("focused link")
}

/// Two-tile layout focused on win-2 (win-1 left of win-2).
fn two_tile() -> (WorldLayout, DomainKey) {
    let mut layout = WorldLayout::new();
    // Fixed-size admission opt-in: overlay observations (fullscreen /
    // maximized) bind advisory-style instead of failing legacy binding,
    // with no effect on ordinary hint-less admissions.
    layout.set_fixed_size_admission(true);
    admit(&mut layout, "win-1", "out-1", "ws-1");
    admit(&mut layout, "win-2", "out-1", "ws-1");
    let domain = key("out-1", "ws-1");
    assert_eq!(focused_window(&layout, &domain).0, "win-2");
    (layout, domain)
}

#[test]
fn admission_over_fullscreen_denied_keeps_slot_and_focus() {
    // Host denied activation: C arrives while native focus stays on
    // fullscreen B. B keeps its slot and focus.
    let (mut layout, domain) = two_tile();
    let leaf_b = leaf_of(&layout, &domain, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), true, false),
    );
    let o0 = obs_for(
        &layout,
        &domain,
        vec![tiled("win-c", "out-1", "ws-1")],
        &flags,
    );
    let counts = layout
        .converge_domain(&domain, &o0, Some(&win("win-2")), &BTreeSet::new())
        .expect("admission converges");
    assert_eq!(counts.admitted, 1);
    assert_eq!(
        leaf_of(&layout, &domain, "win-2"),
        leaf_b,
        "B retains its tile slot"
    );
    assert_eq!(
        focused_window(&layout, &domain).0,
        "win-2",
        "denied C keeps B focused"
    );
    assert!(
        layout.is_tiled_window(&domain, &win("win-c")),
        "C is structurally admitted behind the overlay"
    );
}

#[test]
fn admission_over_fullscreen_approved_keeps_slot_moves_focus() {
    // Host approved activation: C arrives with native focus on C. B keeps
    // its slot while focus follows the host to the newcomer.
    let (mut layout, domain) = two_tile();
    let leaf_b = leaf_of(&layout, &domain, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), true, false),
    );
    let o0 = obs_for(
        &layout,
        &domain,
        vec![tiled("win-c", "out-1", "ws-1")],
        &flags,
    );
    let counts = layout
        .converge_domain(&domain, &o0, Some(&win("win-c")), &BTreeSet::new())
        .expect("admission converges");
    assert_eq!(counts.admitted, 1);
    assert_eq!(
        leaf_of(&layout, &domain, "win-2"),
        leaf_b,
        "B retains its tile slot"
    );
    assert_eq!(
        focused_window(&layout, &domain).0,
        "win-c",
        "approved C takes focus"
    );
}

#[test]
fn admission_over_maximized_denied_keeps_slot_and_focus() {
    // Maximized native activation ownership: same structural rule as the
    // fullscreen leg with the maximized overlay.
    let (mut layout, domain) = two_tile();
    let leaf_b = leaf_of(&layout, &domain, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), false, true),
    );
    let o0 = obs_for(
        &layout,
        &domain,
        vec![tiled("win-c", "out-1", "ws-1")],
        &flags,
    );
    let counts = layout
        .converge_domain(&domain, &o0, Some(&win("win-2")), &BTreeSet::new())
        .expect("admission converges");
    assert_eq!(counts.admitted, 1);
    assert_eq!(
        leaf_of(&layout, &domain, "win-2"),
        leaf_b,
        "B retains its tile slot"
    );
    assert_eq!(
        focused_window(&layout, &domain).0,
        "win-2",
        "denied C keeps B focused"
    );
}

/// Occupied-target layout focused on win-2 (win-t waits on ws-b).
fn send_fixture() -> (WorldLayout, DomainKey, DomainKey) {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    admit(&mut layout, "win-t", "out-1", "ws-b");
    admit(&mut layout, "win-1", "out-1", "ws-a");
    admit(&mut layout, "win-2", "out-1", "ws-a");
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    assert_eq!(focused_window(&layout, &source).0, "win-2");
    (layout, source, target)
}

#[test]
fn fullscreen_mover_sends_with_leaf_preserved_and_follow_focus() {
    // D28: an explicit send of a retained fullscreen tiled mover plans the
    // ordinary transfer with its leaf preserved and follow focus intact;
    // core performs no unfullscreen.
    let (mut layout, source, target) = send_fixture();
    let leaf = leaf_of(&layout, &source, "win-2");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-a"), true, false),
    );
    let mut combined: Vec<EngineWindow> = obs_for(&layout, &source, vec![], &flags)
        .iter()
        .map(eng_from_observed)
        .collect();
    combined.extend(
        obs_for(&layout, &target, vec![], &BTreeMap::new())
            .iter()
            .map(eng_from_observed),
    );
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let subject = win("win-2");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &combined,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subject, true, false)
        .expect("fullscreen send plans the ordinary transfer");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(
        leaf_of(&layout, &target, "win-2"),
        leaf,
        "mover keeps its leaf identity across the send"
    );
    assert_eq!(
        layout.focus_leaf(&target),
        Some(&leaf),
        "follow moves focus into the target and keeps the mover leaf"
    );
    assert_eq!(outcome.focus_leaf, Some(leaf));
    assert!(
        outcome
            .geometry
            .iter()
            .any(|g| g.window.0 == "win-2" && g.workspace == target.workspace),
        "mover keeps a retained target allocation (overlay, never removed)"
    );
    assert!(
        outcome.geometry.iter().any(|g| g.window.0 == "win-t"),
        "occupied target geometry covers the existing tile"
    );
}

#[test]
fn fullscreen_mover_stay_falls_back_to_source_mru() {
    let (mut layout, source, target) = send_fixture();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-a"), true, false),
    );
    let mut combined: Vec<EngineWindow> = obs_for(&layout, &source, vec![], &flags)
        .iter()
        .map(eng_from_observed)
        .collect();
    combined.extend(
        obs_for(&layout, &target, vec![], &BTreeMap::new())
            .iter()
            .map(eng_from_observed),
    );
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let subject = win("win-2");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &combined,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subject, false, false)
        .expect("fullscreen stay-send plans");
    assert_eq!(
        outcome.stay_focus_window,
        Some(win("win-1")),
        "stay falls back to the source MRU survivor"
    );
    assert_eq!(
        focused_window(&layout, &source).0,
        "win-1",
        "source retains MRU survivor focus"
    );
}

#[test]
fn close_fullscreen_removal_refills_survivor_and_focus() {
    // CLOSE-05 fullscreen leg: closing focused fullscreen B removes the
    // tile, survivor A refills the exact domain rect, focus falls to A.
    let (mut layout, domain) = two_tile();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), true, false),
    );
    let o_fs = obs_for(&layout, &domain, vec![], &flags);
    layout
        .converge_domain(&domain, &o_fs, Some(&win("win-2")), &BTreeSet::new())
        .expect("fullscreen state converged");

    // Close win-2 (absent from observed and not in hidden).
    let o_closed = vec![tiled("win-1", "out-1", "ws-1")];
    let counts = layout
        .converge_domain(&domain, &o_closed, None, &BTreeSet::new())
        .expect("close converges");
    assert_eq!(counts.removed, 1);
    let geometry = layout
        .project_domain_geometry(&domain, &BTreeMap::new())
        .expect("projection");
    assert!(
        geometry.iter().all(|g| g.window.0 != "win-2"),
        "removed overlay takes no allocation"
    );
    assert_eq!(geometry.len(), 1, "geometry covers exactly the survivor");
    assert_eq!(
        geometry[0].rect,
        bounds(),
        "sole survivor refills the exact domain rect"
    );
    assert!(
        !layout.is_tiled_window(&domain, &win("win-2"))
            && !layout.is_exception(&domain, &win("win-2")),
        "overlay is gone with no restore"
    );
    assert_eq!(
        focused_window(&layout, &domain).0,
        "win-1",
        "sole survivor takes focus"
    );
    assert_eq!(
        layout.tiled_count(&domain),
        1,
        "survivor alone refills the domain"
    );
}

#[test]
fn close_maximized_removal_refills_survivor_and_focus() {
    // CLOSE-05 maximized leg: same removal/refill/focus rule.
    let (mut layout, domain) = two_tile();
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-2".to_owned(),
        flagged(&tiled("win-2", "out-1", "ws-1"), false, true),
    );
    let o_max = obs_for(&layout, &domain, vec![], &flags);
    layout
        .converge_domain(&domain, &o_max, Some(&win("win-2")), &BTreeSet::new())
        .expect("maximized state converged");

    let o_closed = vec![tiled("win-1", "out-1", "ws-1")];
    let counts = layout
        .converge_domain(&domain, &o_closed, None, &BTreeSet::new())
        .expect("close converges");
    assert_eq!(counts.removed, 1);
    let geometry = layout
        .project_domain_geometry(&domain, &BTreeMap::new())
        .expect("projection");
    assert!(
        geometry.iter().all(|g| g.window.0 != "win-2"),
        "removed overlay takes no allocation"
    );
    assert_eq!(geometry.len(), 1, "geometry covers exactly the survivor");
    assert_eq!(
        geometry[0].rect,
        bounds(),
        "sole survivor refills the exact domain rect"
    );
    assert!(
        !layout.is_tiled_window(&domain, &win("win-2"))
            && !layout.is_exception(&domain, &win("win-2")),
        "overlay is gone with no restore"
    );
    assert_eq!(
        focused_window(&layout, &domain).0,
        "win-1",
        "sole survivor takes focus"
    );
    assert_eq!(
        layout.tiled_count(&domain),
        1,
        "survivor alone refills the domain"
    );
}
