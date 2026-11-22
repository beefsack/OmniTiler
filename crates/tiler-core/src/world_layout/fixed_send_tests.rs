//! Automatic fixed-size floats send everywhere with no `fixed_send_admission`
//! opt-in. The automatic mover (retained classifier mark or wire `fixed_auto`
//! flag) must be slotless-send eligible by default; the explicit tile win
//! (`fixed_suppress`), sticky, and dual-overlay shapes must still refuse.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
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

fn obs_tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
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
        .map(|w| obs_tiled(&w.0, &domain.output.0, &domain.workspace.0))
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut row = obs_tiled(&w.0, &domain.output.0, &domain.workspace.0);
        row.floating = flags.floating;
        row.fullscreen = flags.fullscreen;
        row.maximized = flags.maximized;
        row.sticky = flags.sticky;
        windows.push(row);
    }
    windows
}

fn admit_tiled(layout: &mut WorldLayout, w: &str, o: &str, ws: &str) {
    let k = key(o, ws);
    if !layout.contains_domain(&k) {
        layout
            .domains
            .insert(k.clone(), DomainLayout::new(bounds(), 0, 0));
    }
    let mut observed = domain_observed(layout, &k);
    observed.push(obs_tiled(w, o, ws));
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

fn pair_observed(layout: &WorldLayout, a: &DomainKey, b: &DomainKey) -> Vec<EngineWindow> {
    domain_observed(layout, a)
        .into_iter()
        .chain(domain_observed(layout, b))
        .map(|o| eng_from_observed(&o))
        .collect()
}

/// Intentional float with a retained automatic-fixed classifier mark, built
/// like production: fixed-size admission on, adapter-asserted automatic origin
/// converged, wire flag not carried afterwards.
fn automatic_mark_fixture() -> WorldLayout {
    let mut layout = WorldLayout::new();
    admit_tiled(&mut layout, "win-s", "out-1", "ws-a");
    admit_tiled(&mut layout, "win-t", "out-1", "ws-b");
    admit_tiled(&mut layout, "win-f", "out-1", "ws-a");
    floatify(&mut layout, "win-f", "out-1", "ws-a");
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

#[test]
fn automatic_fixed_float_send_allowed_without_opt_in() {
    // Retained-mark automatic mover sends with no opt-in: eligible and the
    // workspace send plans slotless MoveDeferred.
    let mut layout = automatic_mark_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let observed = pair_observed(&layout, &source, &target);
    let subject = win("win-f");
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &subject, true, false)
        .expect("automatic send plans with no opt-in");
    assert_eq!(
        outcome.kind,
        PairSendKind::Deferred,
        "automatic send carries the deferred transfer"
    );
    assert!(
        outcome.geometry.iter().all(|g| g.window != subject),
        "slotless mover takes no allocation"
    );
    assert!(layout.is_exception(&target, &subject));
    assert!(layout.is_automatic_fixed_float(&target, &subject));

    // Wire-flag-only automatic origin (no retained mark) is eligible too.
    let mut wire_only = WorldLayout::new();
    admit_tiled(&mut wire_only, "win-s", "out-1", "ws-a");
    admit_tiled(&mut wire_only, "win-t", "out-1", "ws-b");
    admit_tiled(&mut wire_only, "win-f", "out-1", "ws-a");
    floatify(&mut wire_only, "win-f", "out-1", "ws-a");
    let mut wire_obs = pair_observed(&wire_only, &source, &target);
    wire_obs
        .iter_mut()
        .find(|w| w.window == subject)
        .expect("mover observed")
        .fixed_auto = true;
    let wire_ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &wire_obs,
        closed: &[],
    };
    let wire_outcome = wire_only
        .run_pair_send(&wire_ctx, &subject, true, false)
        .expect("wire-flag automatic mover eligible with no opt-in");
    assert_eq!(wire_outcome.kind, PairSendKind::Deferred);
    assert!(wire_outcome.geometry.iter().all(|g| g.window != subject));
}

#[test]
fn automatic_send_safety_refusals_preserved() {
    // Explicit tile win, sticky, and dual overlays still refuse the slotless
    // transfer even though automatic origin no longer gates.
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let src_dom = dom("out-1", "ws-a");
    let dst_dom = dom("out-1", "ws-b");
    let subject = win("win-f");

    // 1. Explicit tile win (`fixed_suppress`) refuses.
    let mut suppressed_layout = automatic_mark_fixture();
    let mut suppressed = pair_observed(&suppressed_layout, &source, &target);
    suppressed
        .iter_mut()
        .find(|w| w.window == subject)
        .expect("mover observed")
        .fixed_suppress = true;
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &suppressed,
        closed: &[],
    };
    assert_eq!(
        suppressed_layout.run_pair_send(&ctx, &subject, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "explicit tile win still ineligible"
    );
    assert!(suppressed_layout.is_exception(&source, &subject));

    // 2. Sticky refuses.
    let mut sticky_layout = automatic_mark_fixture();
    let mut sticky = pair_observed(&sticky_layout, &source, &target);
    sticky
        .iter_mut()
        .find(|w| w.window == subject)
        .expect("mover observed")
        .sticky = true;
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &sticky,
        closed: &[],
    };
    assert_eq!(
        sticky_layout.run_pair_send(&ctx, &subject, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "sticky still ineligible"
    );
    assert!(sticky_layout.is_exception(&source, &subject));

    // 3. Dual overlays refuse.
    let mut dual_layout = automatic_mark_fixture();
    let mut dual = pair_observed(&dual_layout, &source, &target);
    {
        let mover = dual
            .iter_mut()
            .find(|w| w.window == subject)
            .expect("mover observed");
        mover.fullscreen = true;
        mover.maximized = true;
    }
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &dual,
        closed: &[],
    };
    assert_eq!(
        dual_layout.run_pair_send(&ctx, &subject, true, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "dual overlays still ineligible"
    );
    assert!(dual_layout.is_exception(&source, &subject));
}
