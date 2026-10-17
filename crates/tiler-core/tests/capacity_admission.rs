//! Best-effort admission regression tests (D1-D10, selected 2026-10-17).
//!
//! Core Session/Engine admission plus geometric repair: ordinary anchor
//! first, D2 fallback ranking (largest area with DFS ties, closest rings,
//! no-search), D3 float/overlap, B6 preservation, diagnostic capacity
//! origin, admission-sequence victim order, D7 repair with protections,
//! startup mixed rosters, replay/close/explicit-retry stability, and no
//! settings reflow. KDE/protocol runtime integration lives in the next
//! unit; this file stays portable and headless.
//! Suite layout (parent holds every shared helper; children hold only tests):
//! - `session_policy`: D2/D3/D7 session policy, fallback ranking, reflow-freeness.
//! - `engine_admission`: engine seeds, newcomer projection, float parity/persistence.
//! - `cross_output_send`: R4/send cross-output exhaustion, overlap, drag agreement.
//! - `send_float_order`: send-float results and admission-order stability.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::admission::{AnchorFallback, NoSpaceResult};
use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecyclePostObservation, Observation,
};
use tiler_core::directional::{Node, NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, Session, SessionCommand,
    SessionObservation, SessionPlan,
};
use tiler_core::size_hints::WindowSizeHints;

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}

fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}

fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
}

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

fn echo_observation(session: &Session, echo: &[(&str, bool)]) -> SessionObservation {
    // Observation where named windows echo a possibly-stale floating
    // flag (adapter lag): capacity floats may read floating:false
    // while core still owns the float classification.
    let mut obs = complete_observation(session, Vec::new());
    for entry in obs.windows.iter_mut() {
        if let Some((_, floating)) = echo.iter().find(|(w, _)| *w == entry.window.0) {
            entry.floating = *floating;
        }
    }
    // Brand-new windows are absent here; callers extend as needed.
    obs
}

fn complete_observation(session: &Session, extra: Vec<ObservedWindow>) -> SessionObservation {
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| ObservedWindow {
            window: link.window.clone(),
            output: link.output.clone(),
            workspace: link.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        })
        .collect();
    windows.extend(session.exception_observed());
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            session.accepted_revision(),
            100 + session.accepted_revision(),
        ),
        windows,
    }
}

fn admit_command(window: &str, output: &str, workspace: &str, placement: Rect) -> SessionCommand {
    SessionCommand::Admit {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: placement,
        suppress_fixed_float: false,
    }
}

fn commit_plan(session: &mut Session, plan: &SessionPlan, corr: &str) {
    let base = plan.dispatch.base_revision;
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 200 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("commit");
}

fn admit_and_commit(
    session: &mut Session,
    window: &str,
    output: &str,
    workspace: &str,
    placement: Rect,
    corr: &str,
) -> SessionPlan {
    let obs = complete_observation(session, vec![tiled_observed(window, output, workspace)]);
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &admit_command(window, output, workspace, placement),
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("admit {window} proposes: {e:?}"));
    assert_eq!(plan.dispatch.base_revision, base);
    commit_plan(session, &plan, corr);
    plan
}

fn tiled_windows(session: &Session) -> Vec<String> {
    let mut out: Vec<String> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .collect();
    out.sort();
    out
}

/// Sibling leaves of `leaf` under its immediate parent group, if any.
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

fn leaf_of(session: &Session, window: &str) -> NodeId {
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| link.window.0 == window)
        .unwrap_or_else(|| panic!("{window} tiled"))
        .leaf
        .clone()
}

// Suite modules (each child holds only `#[test]` fns and uses the shared
// helpers above via `super::*`; no helper is duplicated in any child).
// Integration-test targets resolve `mod` siblings against `tests/`, so the
// child paths are explicit.
#[path = "capacity_admission/cross_output_send.rs"]
mod cross_output_send;
#[path = "capacity_admission/engine_admission.rs"]
mod engine_admission;
#[path = "capacity_admission/send_float_order.rs"]
mod send_float_order;
#[path = "capacity_admission/session_policy.rs"]
mod session_policy;

fn engine_window(
    id: &str,
    output: &str,
    workspace: &str,
    rect: Rect,
) -> tiler_core::seed::EngineWindow {
    tiler_core::seed::EngineWindow {
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

fn reconcile_event(
    owner: &OwnerId,
    generation: &GenerationId,
    domain: &OutputDomain,
    outer_gap: i32,
    focused: &str,
    windows: Vec<tiler_core::seed::EngineWindow>,
    corr: &str,
) -> tiler_core::boundary::CoreEvent {
    use tiler_core::boundary::{CoreCommand, CoreEvent};
    CoreEvent {
        owner: owner.clone(),
        generation: generation.clone(),
        correlation: correlation(corr),
        revision: 0,
        fingerprint: 7,
        domain: domain.clone(),
        domain_key: domain.key(),
        outer_gap,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Reconcile,
    }
}

fn tiled_labels(session: &Session) -> Vec<String> {
    let mut out: Vec<String> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .collect();
    out.sort();
    out
}
