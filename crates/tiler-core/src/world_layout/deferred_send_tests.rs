//! D28 slotless deferred transfer (born-fullscreen hold) and D16 ordinary
//! intentional-float + single-overlay combo + automatic-fixed workspace/output
//! sends over `WorldLayout::run_pair_send`.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{LayoutFailure, PairOperationFailure, PairSendKind};
use super::pair_operation::PairOperationContext;
use super::state::{DomainLayout, WorldLayout};

fn bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 1200,
        h: 800,
    }
}

fn float_rect() -> Rect {
    Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
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

fn key(o: &str, ws: &str) -> DomainKey {
    DomainKey {
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
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

/// Native send-route shape of the held mover: NOT floating (the
/// workspace-send entry only maps sticky to floating), still fullscreen.
fn held_native(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(w),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: true,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
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

fn domain_observed(layout: &WorldLayout, domain: &DomainKey) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(domain)
        .map(|w| tiled(&w.0, &domain.output.0, &domain.workspace.0))
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut row = tiled(&w.0, &domain.output.0, &domain.workspace.0);
        row.floating = flags.floating;
        row.fullscreen = flags.fullscreen;
        row.maximized = flags.maximized;
        row.sticky = flags.sticky;
        windows.push(row);
    }
    windows
}

fn pair_observed(layout: &WorldLayout, a: &DomainKey, b: &DomainKey) -> Vec<EngineWindow> {
    domain_observed(layout, a)
        .into_iter()
        .chain(domain_observed(layout, b))
        .map(|o| eng_from_observed(&o))
        .collect()
}

fn admit(layout: &mut WorldLayout, w: &str, o: &str, ws: &str) {
    let k = key(o, ws);
    if !layout.contains_domain(&k) {
        layout
            .domains
            .insert(k.clone(), DomainLayout::new(bounds(), 0, 0));
    }
    let mut observed = domain_observed(layout, &k);
    observed.push(tiled(w, o, ws));
    layout
        .admit_window(&k, &win(w), bounds(), false, false, &observed)
        .expect("admit");
}

fn floatify(layout: &mut WorldLayout, w: &str, o: &str, ws: &str) {
    let k = key(o, ws);
    let observed = domain_observed(layout, &k);
    layout
        .toggle_float(&k, &win(w), Some(float_rect()), &observed)
        .expect("floatify");
}

fn leaf_of(layout: &WorldLayout, domain: &DomainKey, window: &str) -> NodeId {
    layout
        .bindings(domain)
        .find_map(|(w, leaf)| (w.0 == window).then(|| leaf.clone()))
        .expect("link")
}

/// Slotless hold fixture: win-s tiled on ws-a, win-t tiled on ws-b, win-f
/// born fullscreen held slotless on ws-a.
fn hold_fixture() -> WorldLayout {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    admit(&mut layout, "win-s", "out-1", "ws-a");
    admit(&mut layout, "win-t", "out-1", "ws-b");
    let source = key("out-1", "ws-a");
    let mut windows = domain_observed(&layout, &source);
    windows.push(held_native("win-f", "out-1", "ws-a"));
    let counts = layout
        .converge_domain(&source, &windows, None, &BTreeSet::new())
        .expect("hold converges");
    assert_eq!(counts.admitted, 0, "held mover takes no slot");
    assert!(
        layout.is_exception(&source, &win("win-f")),
        "mover held slotless"
    );
    layout
}

/// Ordinary-float fixture: win-s tiled on ws-a, win-t tiled on ws-b,
/// win-f intentionally floated on ws-a with the pinned size.
fn float_fixture() -> WorldLayout {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-s", "out-1", "ws-a");
    admit(&mut layout, "win-t", "out-1", "ws-b");
    admit(&mut layout, "win-f", "out-1", "ws-a");
    floatify(&mut layout, "win-f", "out-1", "ws-a");
    let source = key("out-1", "ws-a");
    assert!(
        layout.is_exception(&source, &win("win-f")),
        "mover floats slotless"
    );
    assert_eq!(
        layout.exception_float_geometry(&source, &win("win-f")),
        Some(float_rect()),
        "float keeps its size"
    );
    assert_eq!(
        layout.retained_float_geometry(&source, &win("win-f")),
        Some(float_rect()),
        "restart float memory staged"
    );
    layout
}

/// Automatic-float fixture with a real retained classifier mark: the
/// intentional float adopts adapter-asserted automatic origin through
/// observation convergence.
fn automatic_fixture() -> WorldLayout {
    let mut layout = float_fixture();
    layout.set_fixed_size_admission(true);
    let source = key("out-1", "ws-a");
    let mut windows = domain_observed(&layout, &source);
    windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_auto = true;
    layout
        .converge_domain(&source, &windows, None, &BTreeSet::new())
        .expect("adopt automatic origin");
    assert!(
        layout.is_automatic_fixed_float(&source, &win("win-f")),
        "classifier mark retained"
    );
    layout
}

fn send_from_ws_a(
    layout: &mut WorldLayout,
    subject: &str,
    follow: bool,
) -> Result<super::outcome::PairSendOutcome, PairOperationFailure> {
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let observed = pair_observed(layout, &source, &target);
    let subj = win(subject);
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    layout.run_pair_send(&ctx, &subj, follow, false)
}

// --- Slotless (D28) tests ---

#[test]
fn slotless_follow_transfers_homing_without_topology_or_reflow() {
    let mut layout = hold_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();

    let outcome = send_from_ws_a(&mut layout, "win-f", true)
        .expect("slotless follow plans the deferred transfer");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.window, win("win-f"));
    assert_eq!(
        (
            outcome.source_output.0.as_str(),
            outcome.source_workspace.0.as_str()
        ),
        ("out-1", "ws-a")
    );
    assert_eq!(
        (
            outcome.target_output.0.as_str(),
            outcome.target_workspace.0.as_str()
        ),
        ("out-1", "ws-b")
    );

    assert_eq!(
        layout.tree(&source).cloned(),
        tree_before_a,
        "source never reflows"
    );
    assert_eq!(
        layout.tree(&target).cloned(),
        tree_before_b,
        "target never reflows"
    );
    assert!(!layout.is_exception(&source, &win("win-f")));
    assert!(
        layout.is_exception(&target, &win("win-f")),
        "mover stays slotless"
    );
    let flags = layout
        .exception_flags(&target, &win("win-f"))
        .expect("mover exception");
    assert!(flags.floating, "hold shape preserved");
    assert!(outcome.focus_leaf.is_some());
    assert_eq!(
        layout.focus_leaf(&target),
        Some(&leaf_of(&layout, &target, "win-t")),
        "follow falls back to the target MRU tile"
    );
    assert!(
        outcome.geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation"
    );
    for w in ["win-s", "win-t"] {
        let entry = outcome
            .geometry
            .iter()
            .find(|g| g.window.0 == w)
            .expect("survivor geometry");
        assert_eq!(entry.rect, bounds(), "{w} allocation never reflows");
    }
}

#[test]
fn slotless_stay_falls_back_to_source_mru() {
    let mut layout = hold_fixture();
    let source = key("out-1", "ws-a");
    let survivor_leaf = leaf_of(&layout, &source, "win-s");
    let outcome = send_from_ws_a(&mut layout, "win-f", false).expect("slotless stay plans");
    assert_eq!(
        outcome.kind,
        PairSendKind::Deferred,
        "stay also carries the deferred transfer"
    );
    assert_eq!(
        outcome.stay_focus_window,
        Some(win("win-s")),
        "stay falls back to the source MRU survivor"
    );
    assert_eq!(
        layout.focus_leaf(&source),
        Some(&survivor_leaf),
        "source keeps MRU survivor focus"
    );
    assert!(
        outcome.geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation on stay either"
    );
}

#[test]
fn slotless_first_exit_admits_at_destination() {
    // D5: the first non-fullscreen observation at the destination admits
    // the carried mover there, never back on the source.
    let mut layout = hold_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let outcome = send_from_ws_a(&mut layout, "win-f", true).expect("send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);

    // Still-fullscreen observation at the destination keeps the hold.
    let held = vec![
        held_native("win-f", "out-1", "ws-b"),
        tiled("win-t", "out-1", "ws-b"),
    ];
    let counts = layout
        .converge_domain(&target, &held, Some(&win("win-f")), &BTreeSet::new())
        .expect("destination hold");
    assert_eq!(counts.admitted, 0, "destination hold admits nothing");
    assert!(
        layout.is_exception(&target, &win("win-f")),
        "mover still slotless on the destination"
    );

    // First exit admits at the destination.
    let exit = vec![
        tiled("win-f", "out-1", "ws-b"),
        tiled("win-t", "out-1", "ws-b"),
    ];
    let counts = layout
        .converge_domain(&target, &exit, Some(&win("win-f")), &BTreeSet::new())
        .expect("first exit");
    assert_eq!(counts.admitted, 0, "exit reuses the D5 unfloat path");
    assert!(
        !layout.is_exception(&target, &win("win-f")),
        "exited mover leaves the exception set"
    );
    assert!(
        layout.is_tiled_window(&target, &win("win-f")),
        "exit admits at the destination, never the source"
    );
    assert!(!layout.is_tiled_window(&source, &win("win-f")));
}

#[test]
fn slotless_send_refuses_non_held_members() {
    // Ordinary tiled window plans PairSendKind::Tiled, not Deferred.
    let mut layout = hold_fixture();
    let outcome = send_from_ws_a(&mut layout, "win-s", true).expect("tiled send plans");
    assert_eq!(outcome.kind, PairSendKind::Tiled);

    // Unknown window refuses with FocusMismatch.
    let mut layout = hold_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let observed = pair_observed(&layout, &source, &target);
    let unknown = win("win-zzz");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &unknown,
        observed: &observed,
        closed: &[],
    };
    assert_eq!(
        layout.run_pair_send(&ctx, &unknown, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "unknown window refuses"
    );

    // Fullscreen exit race: in WorldLayout::run_pair_send, converge_pair runs
    // BEFORE deferred_send_eligible, so a mover that exited fullscreen in the
    // observation is converged via D5 exit to tiled and sent as Tiled (the old
    // pre-convergence Session::propose NotTiled race check is retired).
    let mut layout = hold_fixture();
    let mut raced = pair_observed(&layout, &source, &target);
    for w in &mut raced {
        if w.window.0 == "win-f" {
            w.fullscreen = false;
            w.floating = false;
        }
    }
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &raced,
        closed: &[],
    };
    let raced_outcome = layout
        .run_pair_send(&ctx, &subj, true, false)
        .expect("D5 exit converges before send");
    assert_eq!(raced_outcome.kind, PairSendKind::Tiled);
    assert!(layout.is_tiled_window(&target, &subj));
}

// --- Floating deferred (D16) tests ---

#[test]
fn ordinary_float_follow_transfers_homing_with_size_kept() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();

    let outcome =
        send_from_ws_a(&mut layout, "win-f", true).expect("float send plans the deferred transfer");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.window, win("win-f"));
    assert_eq!(
        (
            outcome.source_output.0.as_str(),
            outcome.source_workspace.0.as_str()
        ),
        ("out-1", "ws-a")
    );
    assert_eq!(
        (
            outcome.target_output.0.as_str(),
            outcome.target_workspace.0.as_str()
        ),
        ("out-1", "ws-b")
    );

    assert_eq!(
        layout.tree(&source).cloned(),
        tree_before_a,
        "source never reflows"
    );
    assert_eq!(
        layout.tree(&target).cloned(),
        tree_before_b,
        "target never reflows"
    );
    assert!(!layout.is_exception(&source, &win("win-f")));
    assert!(
        layout.is_exception(&target, &win("win-f")),
        "mover stays floating"
    );
    let flags = layout
        .exception_flags(&target, &win("win-f"))
        .expect("mover exception");
    assert!(flags.floating, "float retained on arrival");
    assert!(!flags.fullscreen && !flags.maximized && !flags.sticky);
    assert_eq!(
        layout.exception_float_geometry(&target, &win("win-f")),
        Some(float_rect()),
        "arrival size kept"
    );
    assert_eq!(
        layout.retained_float_geometry(&target, &win("win-f")),
        Some(float_rect()),
        "restart float memory follows the window"
    );
    assert_eq!(
        layout.retained_float_geometry(&source, &win("win-f")),
        None,
        "no orphaned entry on source"
    );
    assert!(outcome.focus_leaf.is_some());
    assert_eq!(
        layout.focus_leaf(&target),
        Some(&leaf_of(&layout, &target, "win-t")),
        "follow falls back to the target MRU tile"
    );
    assert!(
        outcome.geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation"
    );
    for w in ["win-s", "win-t"] {
        let entry = outcome
            .geometry
            .iter()
            .find(|g| g.window.0 == w)
            .expect("survivor geometry");
        assert_eq!(entry.rect, bounds(), "{w} allocation never reflows");
    }
}

#[test]
fn ordinary_float_stay_falls_back_to_source_mru() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let survivor_leaf = leaf_of(&layout, &source, "win-s");
    let outcome = send_from_ws_a(&mut layout, "win-f", false).expect("stay plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(
        outcome.stay_focus_window,
        Some(win("win-s")),
        "stay falls back to the source MRU survivor"
    );
    assert_eq!(layout.focus_leaf(&source), Some(&survivor_leaf));
    assert!(
        outcome.geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation on stay either"
    );
    assert!(
        layout.is_exception(&target, &win("win-f")),
        "stay still rehomes the mover"
    );
    assert_eq!(
        layout.exception_float_geometry(&target, &win("win-f")),
        Some(float_rect()),
        "stay keeps the size"
    );
}

#[test]
fn ordinary_float_roundtrip_retains_float_both_legs() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();

    // Leg one: ws-a -> ws-b (follow).
    let leg1 = send_from_ws_a(&mut layout, "win-f", true).expect("leg one plans");
    assert_eq!(leg1.kind, PairSendKind::Deferred);
    assert!(layout.is_exception(&target, &win("win-f")));

    // Leg two: ws-b -> ws-a (stay).
    let src_dom = dom("out-1", "ws-b");
    let dst_dom = dom("out-1", "ws-a");
    let observed = pair_observed(&layout, &target, &source);
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let leg2 = layout
        .run_pair_send(&ctx, &subj, false, false)
        .expect("return leg plans");
    assert_eq!(leg2.kind, PairSendKind::Deferred);

    assert!(
        layout.is_exception(&source, &subj),
        "return leg arrives home"
    );
    assert!(
        layout
            .exception_flags(&source, &subj)
            .expect("flags")
            .floating
    );
    assert_eq!(
        layout.exception_float_geometry(&source, &subj),
        Some(float_rect()),
        "size kept across the roundtrip"
    );
    assert_eq!(
        layout.retained_float_geometry(&source, &subj),
        Some(float_rect()),
        "restart memory follows across the roundtrip"
    );
    assert_eq!(layout.tree(&source).cloned(), tree_before_a);
    assert_eq!(layout.tree(&target).cloned(), tree_before_b);
}

#[test]
fn sticky_observed_float_send_refuses_not_tiled() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .sticky = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    assert_eq!(
        layout.run_pair_send(&ctx, &subj, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "sticky send must refuse"
    );
}

#[test]
fn maximized_combo_follow_carries_deferred() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .maximized = true;
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, false)
        .expect("maximized combo send plans the deferred transfer");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(layout.tree(&source).cloned(), tree_before_a);
    assert_eq!(layout.tree(&target).cloned(), tree_before_b);
    assert!(layout.is_exception(&target, &subj));
    let flags = layout.exception_flags(&target, &subj).expect("flags");
    assert!(flags.floating && !flags.sticky);
    assert_eq!(
        layout.exception_float_geometry(&target, &subj),
        Some(float_rect())
    );
    assert_eq!(
        layout.retained_float_geometry(&target, &subj),
        Some(float_rect())
    );
    assert!(outcome.focus_leaf.is_some());
    assert_eq!(
        layout.focus_leaf(&target),
        Some(&leaf_of(&layout, &target, "win-t"))
    );
    assert!(outcome.geometry.iter().all(|g| g.window.0 != "win-f"));
    for w in ["win-s", "win-t"] {
        let entry = outcome
            .geometry
            .iter()
            .find(|g| g.window.0 == w)
            .expect("survivor geometry");
        assert_eq!(entry.rect, bounds());
    }
}

#[test]
fn automatic_fixed_float_send_plans_move_deferred() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_auto = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, false)
        .expect("automatic send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert!(outcome.geometry.iter().all(|g| g.window.0 != "win-f"));
}

#[test]
fn automatic_fixed_float_send_admits_by_default() {
    let mut layout = automatic_fixture();
    let outcome = send_from_ws_a(&mut layout, "win-f", true).expect("automatic send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.window, win("win-f"));
    assert!(outcome.geometry.iter().all(|g| g.window.0 != "win-f"));

    // The explicit tile win (`fixed_suppress`) still refuses.
    let mut suppressed_layout = automatic_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&suppressed_layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_suppress = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    assert_eq!(
        suppressed_layout.run_pair_send(&ctx, &subj, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "explicit tile win still ineligible"
    );
}

#[test]
fn automatic_fixed_float_roundtrip_retains_marker_and_frame() {
    let mut layout = automatic_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();

    // Leg one: ws-a -> ws-b (follow).
    let leg1 = send_from_ws_a(&mut layout, "win-f", true).expect("leg one plans");
    assert_eq!(leg1.kind, PairSendKind::Deferred);
    assert!(leg1.geometry.iter().all(|g| g.window.0 != "win-f"));
    assert!(layout.is_exception(&target, &win("win-f")));
    assert!(layout.is_automatic_fixed_float(&target, &win("win-f")));
    assert_eq!(
        layout.exception_float_geometry(&target, &win("win-f")),
        Some(float_rect())
    );
    assert_eq!(
        layout.retained_float_geometry(&target, &win("win-f")),
        Some(float_rect())
    );

    // Leg two: ws-b -> ws-a (stay).
    let src_dom = dom("out-1", "ws-b");
    let dst_dom = dom("out-1", "ws-a");
    let observed = pair_observed(&layout, &target, &source);
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let leg2 = layout
        .run_pair_send(&ctx, &subj, false, false)
        .expect("return leg plans");
    assert_eq!(leg2.kind, PairSendKind::Deferred);
    assert!(leg2.geometry.iter().all(|g| g.window.0 != "win-f"));
    assert!(layout.is_exception(&source, &subj));
    assert!(layout.is_automatic_fixed_float(&source, &subj));
    assert_eq!(
        layout.exception_float_geometry(&source, &subj),
        Some(float_rect())
    );
    assert_eq!(
        layout.retained_float_geometry(&source, &subj),
        Some(float_rect())
    );
    assert_eq!(layout.tree(&source).cloned(), tree_before_a);
    assert_eq!(layout.tree(&target).cloned(), tree_before_b);
}

#[test]
fn tiled_send_still_plans_move_tiled() {
    let mut layout = float_fixture();
    let outcome = send_from_ws_a(&mut layout, "win-s", true).expect("tiled send still plans");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
}

#[test]
fn ordinary_float_output_send_follow() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-s", "out-1", "ws-a");
    admit(&mut layout, "win-u", "out-2", "ws-a");
    admit(&mut layout, "win-f", "out-1", "ws-a");
    floatify(&mut layout, "win-f", "out-1", "ws-a");
    let source = key("out-1", "ws-a");
    let target = key("out-2", "ws-a");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-2", "ws-a");
    let observed = pair_observed(&layout, &source, &target);
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, true)
        .expect("cross-output float send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.source_output.0, "out-1");
    assert_eq!(outcome.target_output.0, "out-2");
    assert!(layout.is_exception(&target, &subj));
    assert_eq!(
        layout.exception_float_geometry(&target, &subj),
        Some(float_rect())
    );
}

#[test]
fn fullscreen_combo_follow_carries_deferred() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fullscreen = true;
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, false)
        .expect("fullscreen combo send plans the deferred transfer");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(layout.tree(&source).cloned(), tree_before_a);
    assert_eq!(layout.tree(&target).cloned(), tree_before_b);
    assert!(layout.is_exception(&target, &subj));
    let flags = layout.exception_flags(&target, &subj).expect("flags");
    assert!(flags.floating && !flags.sticky);
    assert_eq!(
        layout.exception_float_geometry(&target, &subj),
        Some(float_rect())
    );
    assert_eq!(
        layout.retained_float_geometry(&target, &subj),
        Some(float_rect())
    );
    assert!(outcome.focus_leaf.is_some());
    assert_eq!(
        layout.focus_leaf(&target),
        Some(&leaf_of(&layout, &target, "win-t"))
    );
    assert!(outcome.geometry.iter().all(|g| g.window.0 != "win-f"));
}

#[test]
fn fullscreen_combo_stay_falls_back_to_source_mru() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let survivor_leaf = leaf_of(&layout, &source, "win-s");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fullscreen = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, false, false)
        .expect("combo stay plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.stay_focus_window, Some(win("win-s")));
    assert_eq!(layout.focus_leaf(&source), Some(&survivor_leaf));
    assert!(layout.is_exception(&target, &subj));
    assert_eq!(
        layout.exception_float_geometry(&target, &subj),
        Some(float_rect())
    );
}

#[test]
fn combo_roundtrip_exit_returns_to_floating() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = layout.tree(&source).cloned();
    let tree_before_b = layout.tree(&target).cloned();

    // Leg one: fullscreen combo ws-a -> ws-b (follow).
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut leg1_obs = pair_observed(&layout, &source, &target);
    leg1_obs
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fullscreen = true;
    let subj = win("win-f");
    let ctx1 = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &leg1_obs,
        closed: &[],
    };
    let leg1 = layout
        .run_pair_send(&ctx1, &subj, true, false)
        .expect("combo leg plans");
    assert_eq!(leg1.kind, PairSendKind::Deferred);
    assert!(layout.is_exception(&target, &subj));

    // Leg two: overlay exits native, plain intentional float returns home (stay).
    let leg2_obs = pair_observed(&layout, &target, &source);
    let ctx2 = PairOperationContext {
        source_domain: &dst_dom,
        target_domain: &src_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &leg2_obs,
        closed: &[],
    };
    let leg2 = layout
        .run_pair_send(&ctx2, &subj, false, false)
        .expect("return leg plans");
    assert_eq!(leg2.kind, PairSendKind::Deferred);
    assert!(layout.is_exception(&source, &subj));
    assert!(
        layout
            .exception_flags(&source, &subj)
            .expect("flags")
            .floating
    );
    assert_eq!(
        layout.exception_float_geometry(&source, &subj),
        Some(float_rect())
    );
    assert_eq!(layout.tree(&source).cloned(), tree_before_a);
    assert_eq!(layout.tree(&target).cloned(), tree_before_b);
}

#[test]
fn sticky_combo_send_refuses_not_tiled() {
    for (name, set_overlay) in [("fullscreen", true), ("maximized", false)] {
        let mut layout = float_fixture();
        let source = key("out-1", "ws-a");
        let target = key("out-1", "ws-b");
        let src_dom = dom("out-1", "ws-a");
        let dst_dom = dom("out-1", "ws-b");
        let mut observed = pair_observed(&layout, &source, &target);
        let mover = observed
            .iter_mut()
            .find(|w| w.window.0 == "win-f")
            .expect("mover observed");
        mover.sticky = true;
        if set_overlay {
            mover.fullscreen = true;
        } else {
            mover.maximized = true;
        }
        let subj = win("win-f");
        let ctx = PairOperationContext {
            source_domain: &src_dom,
            target_domain: &dst_dom,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subj,
            observed: &observed,
            closed: &[],
        };
        assert_eq!(
            layout.run_pair_send(&ctx, &subj, true, false),
            Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
                RefusalKind::FocusMismatch
            ))),
            "sticky {name} combo must refuse"
        );
    }
}

#[test]
fn both_overlays_combo_send_refuses() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    let mover = observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed");
    mover.fullscreen = true;
    mover.maximized = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    assert_eq!(
        layout.run_pair_send(&ctx, &subj, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        )))
    );
}

#[test]
fn automatic_combo_send_plans_but_suppress_combo_refuses() {
    let mut layout = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    let mover = observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed");
    mover.fullscreen = true;
    mover.fixed_auto = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, false)
        .expect("automatic combo send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert!(outcome.geometry.iter().all(|g| g.window.0 != "win-f"));

    // The explicit tile win still refuses with exactly one overlay.
    let mut suppressed_layout = float_fixture();
    let mut observed = pair_observed(&suppressed_layout, &source, &target);
    let mover = observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed");
    mover.fullscreen = true;
    mover.fixed_suppress = true;
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    assert_eq!(
        suppressed_layout.run_pair_send(&ctx, &subj, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        )))
    );
}

#[test]
fn combo_empty_target_carries_without_geometry() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-f", "out-1", "ws-a");
    floatify(&mut layout, "win-f", "out-1", "ws-a");
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fullscreen = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, false)
        .expect("empty-target combo send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert!(
        outcome.geometry.is_empty(),
        "no tiled members either side, no allocations"
    );
    assert!(
        outcome.focus_leaf.is_none(),
        "member-less target leaves follow focus null"
    );
    assert!(layout.is_exception(&target, &subj));
    assert_eq!(
        layout.exception_float_geometry(&target, &subj),
        Some(float_rect())
    );
}

#[test]
fn combo_output_send_follow() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-s", "out-1", "ws-a");
    admit(&mut layout, "win-u", "out-2", "ws-a");
    admit(&mut layout, "win-f", "out-1", "ws-a");
    floatify(&mut layout, "win-f", "out-1", "ws-a");
    let source = key("out-1", "ws-a");
    let target = key("out-2", "ws-a");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-2", "ws-a");
    let mut observed = pair_observed(&layout, &source, &target);
    observed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fullscreen = true;
    let subj = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subj,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subj, true, true)
        .expect("cross-output combo send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.source_output.0, "out-1");
    assert_eq!(outcome.target_output.0, "out-2");
    assert!(layout.is_exception(&target, &subj));
    assert_eq!(
        layout.exception_float_geometry(&target, &subj),
        Some(float_rect())
    );
}
