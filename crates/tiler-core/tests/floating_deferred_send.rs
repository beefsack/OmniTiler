//! D16 ordinary intentional-float workspace transfer (option (a), no
//! setting): explicit numbered/relative follow/stay send of an
//! intentional floating window.
//!
//! Session-level contract, reusing the D28 slotless deferred route:
//! the floating mover (slotless exception with retained size)
//! transfers to the target homing with NO tree mutation and NO
//! source/target reflow. The retained record keeps its flags and
//! floating size verbatim, and the window-id-keyed restart float
//! memory follows with no rehoming step. The operation carries
//! window plus source/target identities with NO leaf (never
//! fabricated). Sticky and maximized movers keep refusing as
//! `NotTiled`; tiled sends still plan `MoveTiled`.

use std::collections::BTreeMap;

use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecycleOperation, LifecyclePrecondition,
    Observation,
};
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, Session, SessionCommand,
    SessionObservation,
};

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn corr(v: &str) -> CorrelationId {
    CorrelationId::parse(v).expect("valid")
}
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
fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}
fn obs(s: &Session, windows: Vec<ObservedWindow>) -> SessionObservation {
    SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 100),
        windows,
    }
}
fn snapshot_tiled(s: &Session) -> Vec<ObservedWindow> {
    s.snapshot()
        .windows
        .iter()
        .map(|l| tiled(&l.window.0, &l.output.0, &l.workspace.0))
        .collect()
}
fn live_observation(s: &Session) -> SessionObservation {
    let mut windows = snapshot_tiled(s);
    windows.extend(s.exception_observed());
    obs(s, windows)
}
fn admit(s: &mut Session, w: &str, o: &str, ws: &str, c: &str) {
    let mut windows = snapshot_tiled(s);
    windows.extend(s.exception_observed());
    windows.push(tiled(w, o, ws));
    let cmd = SessionCommand::Admit {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: bounds(),
        suppress_fixed_float: false,
    };
    let plan = s
        .propose(
            &cmd,
            &obs(s, windows),
            &corr(c),
            &LifecycleCapabilities::full(),
        )
        .expect("admit");
    commit(
        s,
        &plan.dispatch.preconditions,
        &plan.dispatch.operation,
        c,
        200,
    );
}
fn commit(
    s: &mut Session,
    preconditions: &[LifecyclePrecondition],
    operation: &LifecycleOperation,
    c: &str,
    fingerprint: u64,
) {
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, fingerprint),
        corr(c),
        true,
        preconditions.to_vec(),
        operation.clone(),
    ))
    .expect("commit");
}
fn floatify(s: &mut Session, w: &str, c: &str) {
    let plan = s
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId(w.to_owned()),
                float_geometry: Some(float_rect()),
            },
            &live_observation(s),
            &corr(c),
            &LifecycleCapabilities::full(),
        )
        .expect("floatify");
    commit(
        s,
        &plan.dispatch.preconditions,
        &plan.dispatch.operation,
        c,
        200,
    );
}
fn leaf_of(s: &Session, window: &str) -> NodeId {
    s.snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == window)
        .expect("link")
        .leaf
        .clone()
}

/// Ordinary-float fixture: win-s tiled on ws-a, win-t tiled on ws-b,
/// win-f intentionally floated on ws-a with the pinned size. Session
/// focus synced to the tiled survivor win-s.
fn float_fixture() -> Session {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![dom("out-1", "ws-a"), dom("out-1", "ws-b")],
    )
    .expect("s");
    admit(&mut s, "win-s", "out-1", "ws-a", "corr-admit-s");
    admit(&mut s, "win-t", "out-1", "ws-b", "corr-admit-t");
    admit(&mut s, "win-f", "out-1", "ws-a", "corr-admit-f");
    floatify(&mut s, "win-f", "corr-float-f");
    assert!(
        s.is_exception(&WindowId("win-f".to_owned())),
        "mover floats slotless"
    );
    assert_eq!(
        s.floating_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "float keeps its size"
    );
    assert_eq!(
        s.retained_float_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "restart float memory staged"
    );
    assert!(
        s.sync_focus_from_window(&key("out-1", "ws-a"), &WindowId("win-s".to_owned())),
        "focus syncs to the tiled survivor"
    );
    s
}

fn send_float(s: &mut Session, follow: bool, c: &str) -> tiler_core::session::SessionPlan {
    let target = key("out-1", "ws-b");
    s.propose(
        &SessionCommand::MoveToWorkspace {
            window: WindowId("win-f".to_owned()),
            target_output: target.output.clone(),
            target_workspace: target.workspace.clone(),
            follow,
        },
        &live_observation(s),
        &corr(c),
        &LifecycleCapabilities::full(),
    )
    .expect("float send plans the deferred transfer")
}

#[test]
fn ordinary_float_follow_transfers_homing_with_size_kept() {
    let mut s = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    assert!(
        s.deferred_send_eligible(
            &WindowId("win-f".to_owned()),
            &source,
            &live_observation(&s).windows,
        ),
        "ordinary intentional float mover is eligible"
    );
    let tree_before_a = s.tree_for(&source).cloned();
    let tree_before_b = s.tree_for(&target).cloned();
    let plan = send_float(&mut s, true, "corr-float-follow");
    match &plan.dispatch.operation {
        LifecycleOperation::MoveDeferred {
            window,
            source_output,
            source_workspace,
            target_output,
            target_workspace,
        } => {
            assert_eq!(window.0, "win-f");
            assert_eq!(
                (source_output.0.as_str(), source_workspace.0.as_str()),
                ("out-1", "ws-a")
            );
            assert_eq!(
                (target_output.0.as_str(), target_workspace.0.as_str()),
                ("out-1", "ws-b")
            );
        }
        other => panic!("float send must plan MoveDeferred, got {other:?}"),
    }
    commit(
        &mut s,
        &plan.dispatch.preconditions,
        &plan.dispatch.operation,
        "corr-float-follow",
        300,
    );
    assert_eq!(
        s.tree_for(&source).cloned(),
        tree_before_a,
        "source never reflows"
    );
    assert_eq!(
        s.tree_for(&target).cloned(),
        tree_before_b,
        "target never reflows"
    );
    assert!(
        s.is_exception(&WindowId("win-f".to_owned())),
        "mover stays floating"
    );
    let record = s
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "win-f")
        .expect("mover exception");
    assert_eq!(record.output.0, "out-1");
    assert_eq!(record.workspace.0, "ws-b", "mover homed on the destination");
    assert!(record.floating, "float retained on arrival");
    assert!(!record.fullscreen && !record.maximized && !record.sticky);
    assert_eq!(
        s.floating_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "arrival size kept"
    );
    assert_eq!(
        s.retained_float_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "restart float memory follows the window, no orphaned entry"
    );
    assert_eq!(
        plan.desired_focus_domain,
        Some(target),
        "follow books the target"
    );
    assert_eq!(
        plan.desired_focus_leaf,
        Some(leaf_of(&s, "win-t")),
        "follow falls back to the target MRU tile"
    );
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation"
    );
    for w in ["win-s", "win-t"] {
        let entry = plan
            .desired_geometry
            .iter()
            .find(|g| g.window.0 == w)
            .expect("survivor geometry");
        assert_eq!(entry.rect, bounds(), "{w} allocation never reflows");
    }
}

#[test]
fn ordinary_float_stay_falls_back_to_source_mru() {
    let mut s = float_fixture();
    let source = key("out-1", "ws-a");
    let survivor_leaf = leaf_of(&s, "win-s");
    let plan = send_float(&mut s, false, "corr-float-stay");
    assert!(
        matches!(
            &plan.dispatch.operation,
            LifecycleOperation::MoveDeferred { .. }
        ),
        "stay also carries the deferred transfer"
    );
    commit(
        &mut s,
        &plan.dispatch.preconditions,
        &plan.dispatch.operation,
        "corr-float-stay",
        300,
    );
    assert_eq!(
        plan.desired_focus_domain,
        Some(source),
        "stay keeps the source"
    );
    assert_eq!(
        plan.desired_focus_leaf,
        Some(survivor_leaf),
        "stay falls back to the source MRU survivor"
    );
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation on stay either"
    );
    let record = s
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "win-f")
        .expect("mover exception");
    assert_eq!(record.workspace.0, "ws-b", "stay still rehomes the mover");
    assert_eq!(
        s.floating_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "stay keeps the size"
    );
}

#[test]
fn ordinary_float_roundtrip_retains_float_both_legs() {
    let mut s = float_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = s.tree_for(&source).cloned();
    let tree_before_b = s.tree_for(&target).cloned();
    // Leg one: ws-a -> ws-b (follow).
    let plan = send_float(&mut s, true, "corr-float-leg-one");
    commit(
        &mut s,
        &plan.dispatch.preconditions,
        &plan.dispatch.operation,
        "corr-float-leg-one",
        300,
    );
    assert_eq!(
        s.exception_observed()
            .into_iter()
            .find(|w| w.window.0 == "win-f")
            .expect("mover exception")
            .workspace
            .0,
        "ws-b",
        "leg one arrives on ws-b"
    );
    // Leg two: ws-b -> ws-a (stay).
    let back = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: source.output.clone(),
                target_workspace: source.workspace.clone(),
                follow: false,
            },
            &live_observation(&s),
            &corr("corr-float-leg-two"),
            &LifecycleCapabilities::full(),
        )
        .expect("return leg plans");
    assert!(
        matches!(
            &back.dispatch.operation,
            LifecycleOperation::MoveDeferred { .. }
        ),
        "return leg also carries the deferred transfer"
    );
    commit(
        &mut s,
        &back.dispatch.preconditions,
        &back.dispatch.operation,
        "corr-float-leg-two",
        400,
    );
    let record = s
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "win-f")
        .expect("mover exception");
    assert_eq!(record.workspace.0, "ws-a", "return leg arrives home");
    assert!(record.floating, "float retained on the return leg");
    assert_eq!(
        s.floating_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "size kept across the roundtrip"
    );
    assert_eq!(
        s.retained_float_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "restart memory follows across the roundtrip"
    );
    assert_eq!(
        s.tree_for(&source).cloned(),
        tree_before_a,
        "source tiles untouched across the roundtrip"
    );
    assert_eq!(
        s.tree_for(&target).cloned(),
        tree_before_b,
        "target tiles untouched across the roundtrip"
    );
}

#[test]
fn sticky_observed_float_send_refuses_not_tiled() {
    let mut s = float_fixture();
    let source = key("out-1", "ws-a");
    let mut windows = live_observation(&s).windows;
    let mover = windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed");
    mover.sticky = true;
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &windows),
        "sticky mover is not eligible"
    );
    let err = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: WorkspaceId("ws-b".to_owned()),
                follow: true,
            },
            &obs(&s, windows),
            &corr("corr-float-sticky"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("sticky send must refuse");
    assert_eq!(err.kind(), "not-tiled", "sticky keeps current behaviour");
}

#[test]
fn maximized_observed_float_send_refuses_not_tiled() {
    let mut s = float_fixture();
    let source = key("out-1", "ws-a");
    let mut windows = live_observation(&s).windows;
    windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .maximized = true;
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &windows),
        "maximized mover is not eligible"
    );
    let err = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: WorkspaceId("ws-b".to_owned()),
                follow: true,
            },
            &obs(&s, windows),
            &corr("corr-float-maximized"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("maximized send must refuse");
    assert_eq!(err.kind(), "not-tiled");
}

#[test]
fn automatic_fixed_float_send_refuses_not_tiled() {
    // Classifier-origin automatic floats are not ordinary intentional
    // floats: the observed-signal half refuses here, while the
    // retained-marker half is pinned by
    // `pair_send_split_preserves_automatic_and_override` in the
    // fixed-size suite (exception observations never carry the
    // marker, so eligibility reads the retained set too).
    let mut s = float_fixture();
    let source = key("out-1", "ws-a");
    let mut windows = live_observation(&s).windows;
    windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_auto = true;
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &windows),
        "automatic mover is not eligible"
    );
    let err = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: WorkspaceId("ws-b".to_owned()),
                follow: true,
            },
            &obs(&s, windows),
            &corr("corr-float-automatic"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("automatic send must refuse");
    assert_eq!(err.kind(), "not-tiled");
}

#[test]
fn tiled_send_still_plans_move_tiled() {
    let mut s = float_fixture();
    let target = key("out-1", "ws-b");
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-s".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: true,
            },
            &live_observation(&s),
            &corr("corr-tiled-sanity"),
            &LifecycleCapabilities::full(),
        )
        .expect("tiled send still plans");
    assert!(
        matches!(
            &plan.dispatch.operation,
            LifecycleOperation::MoveTiled { .. }
        ),
        "tiled sends are unchanged"
    );
}

#[test]
fn ordinary_float_output_send_follow() {
    // Cross-output (relative) send of an ordinary float through the
    // shared implementation with `allow_cross_output`.
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![dom("out-1", "ws-a"), dom("out-2", "ws-a")],
    )
    .expect("s");
    admit(&mut s, "win-s", "out-1", "ws-a", "corr-out-admit-s");
    admit(&mut s, "win-u", "out-2", "ws-a", "corr-out-admit-u");
    admit(&mut s, "win-f", "out-1", "ws-a", "corr-out-admit-f");
    floatify(&mut s, "win-f", "corr-out-float-f");
    let target = key("out-2", "ws-a");
    let plan = s
        .propose(
            &SessionCommand::MoveToOutput {
                window: WindowId("win-f".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: true,
            },
            &live_observation(&s),
            &corr("corr-out-float-follow"),
            &LifecycleCapabilities::full(),
        )
        .expect("cross-output float send plans");
    match &plan.dispatch.operation {
        LifecycleOperation::MoveDeferred {
            window,
            source_output,
            target_output,
            ..
        } => {
            assert_eq!(window.0, "win-f");
            assert_eq!(source_output.0, "out-1");
            assert_eq!(target_output.0, "out-2");
        }
        other => panic!("cross-output float send must plan MoveDeferred, got {other:?}"),
    }
    commit(
        &mut s,
        &plan.dispatch.preconditions,
        &plan.dispatch.operation,
        "corr-out-float-follow",
        300,
    );
    let record = s
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "win-f")
        .expect("mover exception");
    assert_eq!(record.output.0, "out-2", "mover homed on the target output");
    assert!(record.floating, "float retained across outputs");
    assert_eq!(
        s.floating_geometry(&WindowId("win-f".to_owned())),
        Some(float_rect()),
        "size kept across outputs"
    );
}
