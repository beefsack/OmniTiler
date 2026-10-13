//! Complete-observation convergence rows (test-first, authorized).
//!
//! Design: docs/changes/archive/observation-convergence.md (2026-09-25). The Engine
//! converges retained membership/floating state to the complete observation
//! through one explicit Session primitive BEFORE running the ordinary
//! operation; Session propose entry points do NOT converge implicitly.
//!
//! These rows drive `Engine::handle` (focus/reconcile) with skewed complete
//! observations and assert converged replies at advanced base revisions.
//! Pre-implementation they fail: skewed observations refuse as
//! `partial-observation` on the focus path (which then discards and reseeds
//! instead of converging) and reconcile rejects membership mismatch. The
//! exact-match row pins unchanged revision semantics and already passes.

use std::collections::BTreeMap;
use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
use tiler_core::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, Observation, ResizeMode,
};
use tiler_core::directional::{Direction, Node, OutputId, WindowId, WorkspaceId};
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
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
fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
}
fn domain(output: &str, workspace: &str, w: i32, h: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}
fn plain_obs(window: WindowId, output: OutputId, workspace: WorkspaceId) -> ObservedWindow {
    ObservedWindow {
        window,
        output,
        workspace,
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}
fn carried(window: &str, rect: Rect, floating: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect,
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}
fn placement() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    }
}

fn seed_engine() -> (Engine, DomainKey, OutputDomain, u64) {
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 800, 600)],
    )
    .expect("session");
    admit_to(&mut session, "win-a", "out-1", "ws-1", "c-a");
    admit_to(&mut session, "win-b", "out-1", "ws-1", "c-b");
    admit_to(&mut session, "win-c", "out-1", "ws-1", "c-c");
    let base = session.accepted_revision();
    let focus = session.focus();
    assert_eq!(
        focus.1.map(|l| l.0),
        Some("leaf-win-c".to_owned()),
        "seed focuses the last admission"
    );
    let k = key("out-1", "ws-1");
    let domain_state = session
        .domains()
        .iter()
        .find(|d| d.key() == k)
        .expect("domain")
        .clone();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(k.clone(), session, 0);
    (engine, k, domain_state, base)
}

fn core_event(
    domain_state: &OutputDomain,
    k: &DomainKey,
    base: u64,
    focused: &str,
    windows: Vec<EngineWindow>,
    corr: &str,
    command: CoreCommand,
) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: correlation(corr),
        revision: base,
        fingerprint: 999,
        domain: domain_state.clone(),
        domain_key: k.clone(),
        outer_gap: 0,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command,
    }
}

fn focus_command(window: &str, direction: &str) -> CoreCommand {
    CoreCommand::Focus {
        window: window.to_owned(),
        direction: direction.to_owned(),
        cross_output_transfer: false,
        float_subject: false,
    }
}

fn move_command(window: &str, direction: &str) -> CoreCommand {
    CoreCommand::Move {
        window: window.to_owned(),
        direction: direction.to_owned(),
        cross_output_transfer: false,
        same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
    }
}

fn resize_command(window: &str, direction: &str, mode: &str, press_index: u32) -> CoreCommand {
    CoreCommand::Resize {
        window: window.to_owned(),
        direction: direction.to_owned(),
        mode: mode.to_owned(),
        press_index,
    }
}

fn leaves(session: &Session) -> Vec<String> {
    let snap = session.snapshot();
    let view = snap
        .domains
        .iter()
        .find(|d| d.output.0 == "out-1" && d.workspace.0 == "ws-1")
        .expect("domain");
    fn collect(node: &Node, out: &mut Vec<String>) {
        match node {
            Node::Leaf { id } => out.push(id.0.clone()),
            Node::Group { children, .. } => {
                for c in children {
                    collect(c, out);
                }
            }
        }
    }
    match &view.tree {
        None => vec![],
        Some(tree) => {
            let mut out = vec![];
            collect(tree, &mut out);
            out
        }
    }
}

#[test]
fn engine_focus_with_floating_skew_converges_before_op() {
    // Retained tiled win-a observed floating: the Engine converges (removes
    // the win-a leaf, retains a floating exception, advances revision by one)
    // and then runs the ordinary focus at the converged revision. Valid
    // command: focus win-c leftwards to win-b over the H survivors.
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        focus_windows(true),
        "f-conv",
        focus_command("win-c", "left"),
    );
    match engine.handle(&event) {
        CoreReply::FocusDirectional(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "ordinary op runs at the converged revision"
            );
            assert_eq!(plan.to_window.0, "win-b");
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert!(
                ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
                "geometry covers converged survivors {ids:?}"
            );
            assert!(
                !ids.contains(&"win-a".to_string()),
                "converged float absent from tiled geometry"
            );
        }
        other => panic!("converged focus must plan at base+1, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert_eq!(
        retained.accepted_revision(),
        base + 2,
        "convergence (+1) then ordinary op commit (+1), never a reseed reset"
    );
    assert!(
        retained.is_exception(&WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(retained).iter().all(|l| l != "leaf-win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_move_with_floating_skew_converges_before_op() {
    // Same skew as the focus row; ordinary R1 perpendicular move of the
    // focused survivor (win-c down, mirroring the session R1 wrap test).
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        focus_windows(true),
        "m-conv",
        move_command("win-c", "down"),
    );
    match engine.handle(&event) {
        CoreReply::MoveDirectional(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "ordinary op runs at the converged revision"
            );
            assert_eq!(plan.direction, Direction::Down);
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert!(
                ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
                "geometry covers converged survivors {ids:?}"
            );
            assert!(
                !ids.contains(&"win-a".to_string()),
                "converged float absent from tiled geometry"
            );
        }
        other => panic!("converged move must plan at base+1, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert_eq!(
        retained.accepted_revision(),
        base + 2,
        "convergence (+1) then ordinary op commit (+1), never a reseed reset"
    );
    assert!(
        retained.is_exception(&WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(retained).iter().all(|l| l != "leaf-win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_resize_with_floating_skew_converges_before_op() {
    // Same skew; ordinary keyboard resize of the focused survivor (win-c
    // left outwards, mirroring the session horizontal resize test).
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        focus_windows(true),
        "r-conv",
        resize_command("win-c", "left", "outwards", 0),
    );
    match engine.handle(&event) {
        CoreReply::Resize(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "ordinary op runs at the converged revision"
            );
            assert_eq!(plan.mode, Some(ResizeMode::Outwards));
            assert_eq!(
                plan.focus_leaf.0, "leaf-win-c",
                "focus retained on survivor"
            );
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert!(
                ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
                "geometry covers converged survivors {ids:?}"
            );
            assert!(
                !ids.contains(&"win-a".to_string()),
                "converged float absent from tiled geometry"
            );
        }
        other => panic!("converged resize must plan at base+1, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert_eq!(
        retained.accepted_revision(),
        base + 2,
        "convergence (+1) then ordinary op commit (+1), never a reseed reset"
    );
    assert!(
        retained.is_exception(&WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(retained).iter().all(|l| l != "leaf-win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_reconcile_with_missing_member_converges() {
    // Missing tiled win-b: removed using the current post-removal observation;
    // survivor topology/focus preserved, revision advanced, geometry covers
    // the converged plan.
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        strip(&[("win-a", false), ("win-c", false)]),
        "rec-miss",
        CoreCommand::Reconcile,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "converged plan at resulting revision"
            );
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert!(
                ids.contains(&"win-a".to_string()) && ids.contains(&"win-c".to_string()),
                "geometry covers converged survivors {ids:?}"
            );
            assert!(
                !ids.contains(&"win-b".to_string()),
                "removed member absent from geometry"
            );
            assert!(plan.focus_domain.is_some() && plan.focus_leaf.is_some());
        }
        other => panic!("converged reconcile must project survivors, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert_eq!(leaves(retained), vec!["leaf-win-a", "leaf-win-c"]);
}

#[test]
fn engine_reconcile_with_unexpected_new_member_admits() {
    // Unexpected new normal win-n: admitted through normal placement without
    // reseeding survivors; geometry covers the converged plan.
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        strip(&[
            ("win-a", false),
            ("win-b", false),
            ("win-c", false),
            ("win-n", false),
        ]),
        "rec-new",
        CoreCommand::Reconcile,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "converged plan at resulting revision"
            );
            let mut ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            ids.sort();
            assert_eq!(ids, vec!["win-a", "win-b", "win-c", "win-n"]);
            assert!(plan.focus_domain.is_some() && plan.focus_leaf.is_some());
        }
        other => panic!("converged reconcile must admit and project, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert!(
        retained
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "win-a"),
        "survivors not reseeded"
    );
}

#[test]
fn engine_focus_with_exact_match_keeps_revision_semantics() {
    // Exact-match observation: no convergence bump. The ordinary focus runs
    // at the retained revision and only its own commit advances it.
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        focus_windows(false),
        "f-exact",
        focus_command("win-c", "left"),
    );
    match engine.handle(&event) {
        CoreReply::FocusDirectional(plan) => {
            assert_eq!(
                plan.base_revision, base,
                "exact-match runs the op at the retained revision"
            );
            assert_eq!(plan.to_window.0, "win-b");
        }
        other => panic!("exact-match focus must plan, got {other:?}"),
    }
    assert_eq!(
        engine.session(&k).expect("session").accepted_revision(),
        base + 1,
        "only the ordinary op commit advances the revision"
    );
}

#[test]
fn engine_converge_error_returns_typed_rejection_without_reset() {
    // Cross-homed observation (win-a on ws-2): typed `cross-domain-mismatch`
    // rejection, nothing advanced, session stays usable for the next op.
    let (mut engine, k, domain_state, base) = seed_engine();
    let skewed = EngineWindow {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    };
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        vec![
            skewed,
            carried(
                "win-b",
                Rect {
                    x: 10,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                false,
            ),
            carried(
                "win-c",
                Rect {
                    x: 20,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                false,
            ),
        ],
        "f-cross",
        focus_command("win-c", "left"),
    );
    match engine.handle(&event) {
        CoreReply::Rejected { kind, .. } => assert_eq!(
            kind, "cross-domain-mismatch",
            "primitive error maps to its typed rejection"
        ),
        other => panic!("cross-homed observation must reject, got {other:?}"),
    }
    let retained = engine.session(&k).expect("no reset on primitive error");
    assert_eq!(retained.accepted_revision(), base, "error advances nothing");
    assert_eq!(
        leaves(retained),
        vec!["leaf-win-a", "leaf-win-b", "leaf-win-c"]
    );
    // Follow-up exact observation still runs the ordinary op (no reseed).
    let exact = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        focus_windows(false),
        "f-after-error",
        focus_command("win-c", "left"),
    );
    match engine.handle(&exact) {
        CoreReply::FocusDirectional(plan) => {
            assert_eq!(plan.base_revision, base);
            assert_eq!(plan.to_window.0, "win-b");
        }
        other => panic!("session must stay usable after error, got {other:?}"),
    }
}

#[test]
fn engine_reconcile_with_floating_skew_converges() {
    // Retained tiled win-a observed floating: reconcile converges (leaf
    // removed, floating exception retained, revision +1) then projects the
    // converged survivors without `partial-observation` or reset.
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        focus_windows(true),
        "rec-float",
        CoreCommand::Reconcile,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "converged plan at resulting revision"
            );
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert!(
                ids.contains(&"win-b".to_string()) && ids.contains(&"win-c".to_string()),
                "geometry covers converged survivors {ids:?}"
            );
            assert!(
                !ids.contains(&"win-a".to_string()),
                "converged float absent from tiled geometry"
            );
            assert!(plan.focus_domain.is_some() && plan.focus_leaf.is_some());
        }
        other => panic!("converged reconcile must project survivors, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert_eq!(retained.accepted_revision(), base + 1);
    assert!(
        retained.is_exception(&WindowId("win-a".to_owned())),
        "win-a retained as floating exception"
    );
    assert!(
        leaves(retained).iter().all(|l| l != "leaf-win-a"),
        "survivor topology preserved without reseeding"
    );
}

#[test]
fn engine_fresh_mixed_float_and_tiled_reconciles() {
    // Fresh domain with a mixed floating/tiled observation: public reconcile
    // converges the float exception plus the normal admission atomically,
    // replies planned at the converged revision with tiled-only geometry.
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let k = key("out-1", "ws-1");
    let domain_state = domain("out-1", "ws-1", 800, 600);
    let event = core_event(
        &domain_state,
        &k,
        0,
        "win-n",
        strip(&[("win-f", true), ("win-n", false)]),
        "fresh-mix",
        CoreCommand::Reconcile,
    );
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            assert_eq!(
                plan.base_revision, 1,
                "fresh convergence (+1) then idempotent reply at the converged revision"
            );
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert_eq!(
                ids,
                vec!["win-n".to_string()],
                "tiled-only geometry covers the admitted normal {ids:?}"
            );
            assert!(plan.focus_domain.is_some() && plan.focus_leaf.is_some());
        }
        other => panic!("fresh mixed reconcile must plan, got {other:?}"),
    }
    let retained = engine.session(&k).expect("fresh domain retained");
    assert_eq!(retained.accepted_revision(), 1);
    assert!(
        retained.is_exception(&WindowId("win-f".to_owned())),
        "float retained as exception, never tiled"
    );
    assert!(
        retained
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "win-n"),
        "normal admitted"
    );
    assert_eq!(leaves(retained), vec!["leaf-win-n"]);
}
fn admit_to(session: &mut Session, window: &str, output: &str, workspace: &str, corr: &str) {
    // Seed helper only: admits through exact observations (no skew, no
    // convergence asserted). Seeding is setup, not under test.
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain_obs(l.window.clone(), l.output.clone(), l.workspace.clone()))
        .collect();
    windows.extend(session.exception_observed());
    windows.push(plain_obs(
        WindowId(window.to_owned()),
        OutputId(output.to_owned()),
        WorkspaceId(workspace.to_owned()),
    ));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let base = session.accepted_revision();
    let obs = SessionObservation {
        observation: Observation::new(owner(), generation(), base, 100 + base),
        windows,
    };
    let plan = session
        .propose(
            &SessionCommand::Admit {
                window: WindowId(window.to_owned()),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: placement(),
                suppress_fixed_float: false,
            },
            &obs,
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("seed ack");
    session
        .verify_lifecycle(&tiler_core::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 200 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("seed commit");
}

fn carried_in(window: &str, output: &str, workspace: &str, x: i32, floating: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x,
            y: 0,
            w: 10,
            h: 10,
        },
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}

/// Focus-path carried set (membership by id; one layout shared by every
/// focus row so spatial order cannot drift between them).
fn focus_windows(floating_a: bool) -> Vec<EngineWindow> {
    vec![
        carried(
            "win-a",
            Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 600,
            },
            floating_a,
        ),
        carried(
            "win-b",
            Rect {
                x: 400,
                y: 0,
                w: 200,
                h: 600,
            },
            false,
        ),
        carried(
            "win-c",
            Rect {
                x: 600,
                y: 0,
                w: 200,
                h: 600,
            },
            false,
        ),
    ]
}

/// 10x10 carried windows in observation order for reconcile/admit/remove
/// rows (membership by id; observed rects are opaque carried frames).
fn strip(items: &[(&str, bool)]) -> Vec<EngineWindow> {
    items
        .iter()
        .enumerate()
        .map(|(i, (name, floating))| carried_in(name, "out-1", "ws-1", i as i32 * 10, *floating))
        .collect()
}

#[test]
fn engine_paired_focus_with_source_skew_converges() {
    // Paired focus with source membership skew (unexpected win-n): converges,
    // then the ordinary local focus proceeds with survivor topology intact
    // and no pair pending staged.
    let mut source = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-1", "ws-1", 800, 600)],
    )
    .expect("source session");
    admit_to(&mut source, "win-a", "out-1", "ws-1", "p-a");
    admit_to(&mut source, "win-b", "out-1", "ws-1", "p-b");
    let mut target = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![domain("out-2", "ws-1", 800, 600)],
    )
    .expect("target session");
    admit_to(&mut target, "win-t", "out-2", "ws-1", "p-t");
    let source_key = key("out-1", "ws-1");
    let target_key = key("out-2", "ws-1");
    let source_domain = source
        .domains()
        .iter()
        .find(|d| d.key() == source_key)
        .expect("source domain")
        .clone();
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.store_committed(source_key.clone(), source, 0);
    engine.store_committed(target_key.clone(), target, 0);
    // Pair domains carry reciprocal adjacency (retained components stay
    // adjacency-free); same identity/bounds/gap as the retained components.
    let source_pair_domain = OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::from([(Direction::Right, OutputId("out-2".to_owned()))]),
    };
    let target_pair_domain = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::from([(Direction::Left, OutputId("out-1".to_owned()))]),
    };
    let event = CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: correlation("p-focus-skew"),
        revision: 0,
        fingerprint: 999,
        domain: source_domain.clone(),
        domain_key: source_key.clone(),
        outer_gap: 0,
        focused_window: WindowId("win-b".to_owned()),
        windows: vec![
            carried_in("win-a", "out-1", "ws-1", 0, false),
            carried_in("win-b", "out-1", "ws-1", 0, false),
            carried_in("win-n", "out-1", "ws-1", 0, false),
            carried_in("win-t", "out-2", "ws-1", 0, false),
        ],
        directional: Some(vec![
            (source_pair_domain, source_key.clone()),
            (target_pair_domain, target_key.clone()),
        ]),
        directional_target_outer_gap: Some(0),
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Focus {
            window: "win-b".to_owned(),
            direction: "left".to_owned(),
            cross_output_transfer: false,
            float_subject: false,
        },
    };
    match engine.handle(&event) {
        CoreReply::FocusDirectional(plan) => {
            let ids: Vec<String> = plan.geometry.iter().map(|g| g.window.0.clone()).collect();
            assert!(
                ids.contains(&"win-a".to_string())
                    && ids.contains(&"win-b".to_string())
                    && ids.contains(&"win-n".to_string()),
                "geometry covers converged source survivors {ids:?}"
            );
        }
        other => panic!("converged pair focus must plan, got {other:?}"),
    }
    let retained_source = engine.session(&source_key).expect("source persists");
    assert!(
        retained_source
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "win-n"),
        "skewed source member converged into retained topology"
    );
    let source_leaves = leaves(retained_source);
    assert!(
        source_leaves.contains(&"leaf-win-a".to_string())
            && source_leaves.contains(&"leaf-win-b".to_string()),
        "survivor topology persists without reseed {source_leaves:?}"
    );
    let retained_target = engine.session(&target_key).expect("target persists");
    assert!(
        retained_target
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "win-t"),
        "target topology persists"
    );
}

#[test]
fn engine_reconcile_valid_exception_only_domain_projects_empty() {
    // Valid exception-only domain: every known member is a floating
    // exception and no tiled leaves remain. Reconcile with the complete
    // exception-only observation must return an empty projection, never
    // `malformed-topology`.
    let (mut engine, k, domain_state, base) = seed_engine();
    let event = core_event(
        &domain_state,
        &k,
        base,
        "win-c",
        strip(&[("win-a", true), ("win-b", true), ("win-c", true)]),
        "rec-exception-only",
        CoreCommand::Reconcile,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => {
            assert_eq!(
                plan.base_revision,
                base + 1,
                "converged plan at resulting revision"
            );
            assert!(
                plan.geometry.is_empty(),
                "exception-only projects empty geometry, got {:?}",
                plan.geometry
            );
            assert!(
                plan.focus_domain.is_none() && plan.focus_leaf.is_none(),
                "exception-only carries no tiled focus"
            );
        }
        other => panic!("exception-only reconcile must project empty, got {other:?}"),
    }
    let retained = engine.session(&k).expect("retained session survives");
    assert_eq!(retained.accepted_revision(), base + 1);
    for name in ["win-a", "win-b", "win-c"] {
        assert!(
            retained.is_exception(&WindowId(name.to_owned())),
            "{name} retained as floating exception"
        );
    }
    assert!(leaves(retained).is_empty(), "no tiled leaves remain");
}
