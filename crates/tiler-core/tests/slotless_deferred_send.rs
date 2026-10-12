//! D28 slotless deferred transfer (option (a)): explicit numbered/relative
//! follow/stay send of a NEVER-TILED born-fullscreen window.
//!
//! Session-level contract: the held mover (slotless floating exception,
//! still fullscreen) transfers to the target homing with NO tree mutation,
//! NO source admission, and NO source/target reflow. First fullscreen exit
//! admits at the destination via the existing D5 rule. The operation carries
//! window plus source/target identities with NO leaf (never fabricated).

use std::collections::BTreeMap;

use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecycleOperation, Observation,
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
/// Native send-route shape of the held mover: NOT floating (the
/// workspace-send entry only maps sticky to floating), still fullscreen.
fn held_native(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: true,
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
        Observation::new(owner(), generation(), base, 200),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
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

/// Slotless hold fixture: win-s tiled on ws-a, win-t tiled on ws-b, win-f
/// born fullscreen held slotless on ws-a. Session focus synced to win-s.
fn hold_fixture() -> Session {
    let mut s = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![dom("out-1", "ws-a"), dom("out-1", "ws-b")],
    )
    .expect("s");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-s", "out-1", "ws-a", "corr-admit-s");
    admit(&mut s, "win-t", "out-1", "ws-b", "corr-admit-t");
    // Born fullscreen holds slotless: no tiled slot, plain exception.
    let mut windows = snapshot_tiled(&s);
    windows.extend(s.exception_observed());
    windows.push(held_native("win-f", "out-1", "ws-a"));
    let counts = s
        .converge_observation(&obs(&s, windows), None)
        .expect("hold converges");
    assert_eq!(counts.admitted, 0, "held mover takes no slot");
    assert!(
        s.is_exception(&WindowId("win-f".to_owned())),
        "mover held slotless"
    );
    assert!(
        s.sync_focus_from_window(&key("out-1", "ws-a"), &WindowId("win-s".to_owned())),
        "focus syncs to the tiled survivor"
    );
    s
}

fn send_observation(s: &Session) -> SessionObservation {
    obs(
        s,
        vec![
            tiled("win-s", "out-1", "ws-a"),
            held_native("win-f", "out-1", "ws-a"),
            tiled("win-t", "out-1", "ws-b"),
        ],
    )
}

#[test]
fn slotless_follow_transfers_homing_without_topology_or_reflow() {
    let mut s = hold_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let tree_before_a = s.tree_for(&source).cloned();
    let tree_before_b = s.tree_for(&target).cloned();
    assert!(
        s.deferred_send_eligible(
            &WindowId("win-f".to_owned()),
            &source,
            &send_observation(&s).windows,
        ),
        "held born-fullscreen mover is eligible"
    );
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: true,
            },
            &send_observation(&s),
            &corr("corr-slotless-follow"),
            &LifecycleCapabilities::full(),
        )
        .expect("slotless follow plans the deferred transfer");
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
        other => panic!("slotless send must plan MoveDeferred, got {other:?}"),
    }
    assert_eq!(
        s.tree_for(&source).cloned(),
        tree_before_a,
        "source tree untouched"
    );
    // Commit and verify the homing moved with no reflow.
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr("corr-slotless-follow"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 300),
        corr("corr-slotless-follow"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
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
        "mover stays slotless"
    );
    let record = s
        .exception_observed()
        .into_iter()
        .find(|w| w.window.0 == "win-f")
        .expect("mover exception");
    assert_eq!(record.output.0, "out-1");
    assert_eq!(record.workspace.0, "ws-b", "mover homed on the destination");
    assert!(record.floating, "hold shape preserved");
    // Follow books the target MRU tiled fallback (the slotless mover has
    // no leaf to name; native follow runs adapter-side on the mover ref).
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
    // Tiled allocations are exactly retained: survivor and target tile keep
    // their full-domain rects, and the mover takes no allocation.
    assert!(
        plan.desired_geometry.iter().all(|g| g.window.0 != "win-f"),
        "slotless mover takes no allocation"
    );
    let survivor = plan
        .desired_geometry
        .iter()
        .find(|g| g.window.0 == "win-s")
        .expect("survivor geometry");
    assert_eq!(survivor.rect, bounds(), "source allocation never reflows");
    let target_tile = plan
        .desired_geometry
        .iter()
        .find(|g| g.window.0 == "win-t")
        .expect("target geometry");
    assert_eq!(
        target_tile.rect,
        bounds(),
        "target allocation never reflows"
    );
}

#[test]
fn slotless_stay_falls_back_to_source_mru() {
    let mut s = hold_fixture();
    let source = key("out-1", "ws-a");
    let target = key("out-1", "ws-b");
    let survivor_leaf = leaf_of(&s, "win-s");
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: false,
            },
            &send_observation(&s),
            &corr("corr-slotless-stay"),
            &LifecycleCapabilities::full(),
        )
        .expect("slotless stay plans");
    assert!(
        matches!(
            &plan.dispatch.operation,
            LifecycleOperation::MoveDeferred { .. }
        ),
        "stay also carries the deferred transfer"
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
}

#[test]
fn slotless_first_exit_admits_at_destination() {
    // D5: the first non-fullscreen observation at the destination admits
    // the carried mover there, never back on the source.
    let mut s = hold_fixture();
    let target = key("out-1", "ws-b");
    let plan = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: target.output.clone(),
                target_workspace: target.workspace.clone(),
                follow: true,
            },
            &send_observation(&s),
            &corr("corr-slotless-exit-follow"),
            &LifecycleCapabilities::full(),
        )
        .expect("send plans");
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr("corr-slotless-exit-follow"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 300),
        corr("corr-slotless-exit-follow"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
    // Still-fullscreen observation at the destination keeps the hold.
    let held = obs(
        &s,
        vec![
            tiled("win-s", "out-1", "ws-a"),
            held_native("win-f", "out-1", "ws-b"),
            tiled("win-t", "out-1", "ws-b"),
        ],
    );
    let counts = s
        .converge_observation(&held, None)
        .expect("destination hold");
    assert_eq!(counts.admitted, 0, "destination hold admits nothing");
    assert!(
        s.is_exception(&WindowId("win-f".to_owned())),
        "mover still slotless on the destination"
    );
    // First exit admits at the destination.
    let exit = obs(
        &s,
        vec![
            tiled("win-s", "out-1", "ws-a"),
            tiled("win-f", "out-1", "ws-b"),
            tiled("win-t", "out-1", "ws-b"),
        ],
    );
    let counts = s.converge_observation(&exit, None).expect("first exit");
    assert_eq!(counts.admitted, 0, "exit reuses the D5 unfloat path");
    assert!(
        !s.is_exception(&WindowId("win-f".to_owned())),
        "exited mover leaves the exception set"
    );
    let link = s
        .snapshot()
        .windows
        .iter()
        .find(|l| l.window.0 == "win-f")
        .expect("mover tiled")
        .clone();
    assert_eq!(link.output.0, "out-1");
    assert_eq!(
        link.workspace.0, "ws-b",
        "exit admits at the destination, never the source"
    );
}

#[test]
fn slotless_send_refuses_non_held_members() {
    // Ordinary intentional floats, sticky, and maximized members stay
    // ineligible (D16 remains a separate gap): only the held
    // born-fullscreen shape carries.
    let mut s = hold_fixture();
    let source = key("out-1", "ws-a");
    // Intentional float (floating, never fullscreen) is not held-fullscreen.
    assert!(
        !s.deferred_send_eligible(
            &WindowId("win-s".to_owned()),
            &source,
            &send_observation(&s).windows,
        ),
        "tiled survivor is not a deferred mover"
    );
    // Unknown window is not eligible either.
    assert!(
        !s.deferred_send_eligible(
            &WindowId("win-zzz".to_owned()),
            &source,
            &send_observation(&s).windows,
        ),
        "unknown window is not eligible"
    );
    // A fullscreen flag race (mover already exited before dispatch) fails
    // closed as NotTiled: the exception is no longer fullscreen-held.
    let raced = obs(
        &s,
        vec![
            tiled("win-s", "out-1", "ws-a"),
            tiled("win-f", "out-1", "ws-a"),
            tiled("win-t", "out-1", "ws-b"),
        ],
    );
    assert!(
        !s.deferred_send_eligible(&WindowId("win-f".to_owned()), &source, &raced.windows,),
        "exited mover loses eligibility"
    );
    let err = s
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: WindowId("win-f".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: WorkspaceId("ws-b".to_owned()),
                follow: true,
            },
            &raced,
            &corr("corr-slotless-race"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("raced send must refuse");
    assert_eq!(err.kind(), "not-tiled", "race refuses without transfer");
}
