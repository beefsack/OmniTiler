//! Best-effort admission regression tests (D1-D10, selected 2026-10-17).
//!
//! Core [`WorldLayout`] admission plus geometric repair: ordinary anchor
//! first, D2 fallback ranking (largest area with DFS ties, closest rings),
//! D3 capacity float, B6 preservation, diagnostic capacity
//! origin, admission-sequence victim order, D7 repair with protections,
//! startup mixed rosters, replay/close/explicit-retry stability, and no
//! settings reflow.
//! Suite layout (parent holds every shared helper; children hold only tests):
//! - `session_policy`: D2/D3/D7 session policy, fallback ranking, reflow-freeness.
//! - `engine_admission`: engine seeds, newcomer projection, float parity/persistence.
//! - `cross_output_send`: R4/send cross-output exhaustion, drag agreement.
//! - `send_float_order`: send-float results and admission-order stability.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DesiredGeometry, DomainKey, ObservedWindow, OutputDomain};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::WorldLayout;

fn domain(output: &str, workspace: &str, w: i32, h: i32, gap: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap,
        adjacent: BTreeMap::new(),
    }
}

fn placement(w: i32, h: i32) -> Rect {
    Rect { x: 0, y: 0, w, h }
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

fn engine_window(id: &str, output: &str, workspace: &str, rect: Rect) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect,
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

fn complete_observed_for(
    layout: &WorldLayout,
    key: &DomainKey,
    extra: Vec<ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(key)
        .map(|w| ObservedWindow {
            window: w.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        })
        .collect();
    for (win, flags) in layout.exception_windows_with_flags(key) {
        windows.push(ObservedWindow {
            window: win.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            floating: flags.floating,
            fullscreen: flags.fullscreen,
            maximized: flags.maximized,
            sticky: flags.sticky,
            fixed_auto: layout.is_automatic_fixed_float(key, &win),
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        });
    }
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}

fn echo_observed_for(
    layout: &WorldLayout,
    key: &DomainKey,
    echo: &[(&str, bool)],
) -> Vec<ObservedWindow> {
    let mut obs = complete_observed_for(layout, key, Vec::new());
    for entry in obs.iter_mut() {
        if let Some((_, floating)) = echo.iter().find(|(w, _)| *w == entry.window.0) {
            entry.floating = *floating;
        }
    }
    obs
}

fn complete_engine_obs(layout: &WorldLayout, keys: &[&DomainKey]) -> Vec<EngineWindow> {
    let mut windows = Vec::new();
    for key in keys {
        for w in layout.tiled_windows(key) {
            windows.push(engine_window(
                &w.0,
                &key.output.0,
                &key.workspace.0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
            ));
        }
        for (w, flags) in layout.exception_windows_with_flags(key) {
            let rect = layout
                .retained_float_geometry(key, &w)
                .or_else(|| layout.exception_float_geometry(key, &w))
                .unwrap_or(Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                });
            let mut entry = engine_window(&w.0, &key.output.0, &key.workspace.0, rect);
            entry.floating = flags.floating;
            entry.fullscreen = flags.fullscreen;
            entry.maximized = flags.maximized;
            entry.sticky = flags.sticky;
            entry.fixed_auto = layout.is_automatic_fixed_float(key, &w);
            windows.push(entry);
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}

fn admit_into(
    layout: &mut WorldLayout,
    d: &OutputDomain,
    window: &str,
    placement_bounds: Rect,
) -> Vec<DesiredGeometry> {
    let key = d.key();
    let win_id = WindowId(window.to_owned());
    if !layout.contains_domain(&key) {
        let out = layout
            .reconcile_fresh(
                d,
                0,
                &win_id,
                &[engine_window(
                    window,
                    &d.id.0,
                    &d.workspace.0,
                    placement_bounds,
                )],
            )
            .unwrap_or_else(|e| panic!("fresh admit {window}: {e:?}"));
        return out.projection.geometry;
    }
    let obs = complete_observed_for(
        layout,
        &key,
        vec![tiled_observed(window, &d.id.0, &d.workspace.0)],
    );
    layout
        .admit_window(&key, &win_id, placement_bounds, false, false, &obs)
        .unwrap_or_else(|e| panic!("admit {window}: {e:?}"))
        .geometry
}

fn tiled_windows(layout: &WorldLayout) -> Vec<String> {
    let mut out: Vec<String> = layout.all_tiled_windows().map(|w| w.0).collect();
    out.sort();
    out
}

fn tiled_labels(layout: &WorldLayout, key: &DomainKey) -> Vec<String> {
    let mut out: Vec<String> = layout.tiled_windows(key).map(|w| w.0.clone()).collect();
    out.sort();
    out
}

fn leaf_of(layout: &WorldLayout, key: &DomainKey, window: &str) -> NodeId {
    layout
        .bindings(key)
        .find(|(w, _)| w.0 == window)
        .unwrap_or_else(|| panic!("{window} tiled"))
        .1
        .clone()
}

#[allow(dead_code)]
fn siblings_of(tree: &Node, leaf: &NodeId) -> Option<Vec<NodeId>> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { children, .. } => {
            for child in children {
                if child.id() == leaf && matches!(child, Node::Leaf { .. }) {
                    return Some(
                        children
                            .iter()
                            .filter_map(|c| match c {
                                Node::Leaf { id } => Some(id.clone()),
                                Node::Group { .. } => None,
                            })
                            .collect(),
                    );
                }
                if let Some(found) = siblings_of(child, leaf) {
                    return Some(found);
                }
            }
            None
        }
    }
}

// Suite modules (each child holds only `#[test]` fns and uses the shared
// helpers above via `super::*`; no helper is duplicated in any child).
// Integration-test targets resolve `mod` siblings against `tests/`, so the
// child paths are explicit.
#[path = "capacity_admission_tests/cross_output_send.rs"]
mod cross_output_send;
#[path = "capacity_admission_tests/engine_admission.rs"]
mod engine_admission;
#[path = "capacity_admission_tests/send_float_order.rs"]
mod send_float_order;
#[path = "capacity_admission_tests/session_policy.rs"]
mod session_policy;
