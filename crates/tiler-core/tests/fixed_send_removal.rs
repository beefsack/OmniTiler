#![forbid(unsafe_code)]
//! Automatic fixed-size floats send everywhere
//! with no `fixed_send_admission` opt-in. The automatic mover (retained
//! classifier mark or wire `fixed_auto` flag) must be slotless-send eligible
//! by default; the explicit tile win (`fixed_suppress`), sticky, and
//! dual-overlay shapes must still refuse.

use std::collections::BTreeMap;

use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecycleOperation, Observation,
};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
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
fn commit(
    s: &mut Session,
    preconditions: &[tiler_core::contract::LifecyclePrecondition],
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

/// Intentional float with a retained automatic-fixed classifier mark, built
/// exactly like production: fixed-size admission on, adapter-asserted
/// automatic origin converged, wire flag not carried afterwards.
fn automatic_mark_fixture() -> Session {
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
    s.set_fixed_size_admission(true);
    let mut windows = live_observation(&s).windows;
    windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_auto = true;
    s.converge_observation(&obs(&s, windows), None)
        .expect("adopt automatic origin");
    assert!(
        s.is_automatic_fixed_float(&WindowId("win-f".to_owned())),
        "classifier mark retained"
    );
    s
}

#[test]
fn automatic_fixed_float_send_allowed_without_opt_in() {
    // Retained-mark automatic mover sends with no opt-in: eligible and the
    // workspace send plans slotless MoveDeferred.
    let mut s = automatic_mark_fixture();
    let source = key("out-1", "ws-a");
    assert!(
        s.deferred_send_eligible(
            &WindowId("win-f".to_owned()),
            &source,
            &live_observation(&s).windows,
        ),
        "retained-mark automatic mover eligible with no opt-in"
    );
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: WorkspaceId("ws-b".to_owned()),
                follow: true,
            },
            &live_observation(&s),
            &corr("corr-auto-no-opt-in"),
            &LifecycleCapabilities::full(),
        )
        .expect("automatic send plans with no opt-in");
    assert!(
        matches!(
            &plan.dispatch.operation,
            LifecycleOperation::MoveDeferred { .. }
        ),
        "automatic send carries the deferred transfer"
    );
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation"
    );

    // Wire-flag-only automatic origin (no retained mark) is eligible too.
    // Fresh session-free check: ordinary intentional float plus a wire
    // `fixed_auto` flag must not refuse.
    let mut t = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![dom("out-1", "ws-a"), dom("out-1", "ws-b")],
    )
    .expect("s");
    admit(&mut t, "win-s", "out-1", "ws-a", "corr-admit-s");
    admit(&mut t, "win-t", "out-1", "ws-b", "corr-admit-t");
    admit(&mut t, "win-f", "out-1", "ws-a", "corr-admit-f");
    floatify(&mut t, "win-f", "corr-float-f");
    let mut windows = live_observation(&t).windows;
    windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_auto = true;
    assert!(
        t.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &windows),
        "wire-flag automatic mover eligible with no opt-in"
    );
}

#[test]
fn automatic_send_safety_refusals_preserved() {
    // Explicit tile win, sticky, and dual overlays still refuse the slotless
    // transfer even though automatic origin no longer gates.
    let s = automatic_mark_fixture();
    let source = key("out-1", "ws-a");

    let mut suppressed = live_observation(&s).windows;
    suppressed
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .fixed_suppress = true;
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &suppressed),
        "explicit tile win still ineligible"
    );

    let mut sticky = live_observation(&s).windows;
    sticky
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("mover observed")
        .sticky = true;
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &sticky),
        "sticky still ineligible"
    );

    let mut dual = live_observation(&s).windows;
    {
        let mover = dual
            .iter_mut()
            .find(|w| w.window.0 == "win-f")
            .expect("mover observed");
        mover.fullscreen = true;
        mover.maximized = true;
    }
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &dual),
        "dual overlays still ineligible"
    );
}
