//! Detached fullscreen return: grant eligibility, exact/redundant
//! selection, per-output settlement, lost-output termination, and the
//! typed physical view sampler.
//!
//! Every report is staged through the real v4 intake. Retained joins
//! (mapping, views, membership, native bindings) are seeded directly
//! because the accepted-run domain assembly is not a test seam; the
//! classification, native and attempt lanes under test come from the real
//! capture.

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId};
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_members::MemberState;
use crate::world_report::{
    DetachedRef, FrameSetter, NativeObservation, NativeOccupancy, NativeWorkspaceFact,
    TransferKind, ViewAttempt, ViewRef, WindowFact, WorldCommand, WorldReportV4,
};
use crate::world_v4_support_tests::{accepted, fact, report, screen};

fn detached_caps() -> HostCapabilities {
    HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        fullscreen: true,
        detached_fullscreen_view: true,
        ..HostCapabilities::default()
    }
}

fn detached_engine(shared: bool) -> WorldEngine {
    let values = if shared {
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )]
    } else {
        Vec::new()
    };
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        detached_caps(),
        values,
    )
    .expect("generation")
}

fn global_unique_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        detached_caps(),
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("global-unique".to_owned()),
        )],
    )
    .expect("generation")
}

fn out(id: &str) -> OutputId {
    OutputId(id.to_owned())
}

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: out(output),
        workspace: crate::directional::WorkspaceId(workspace.to_owned()),
    }
}

fn member(engine: &mut WorldEngine, window: &str, output: &str, workspace: &str) {
    engine.world.workspace.members.insert(
        WindowId(window.to_owned()),
        MemberState {
            home: key(output, workspace),
            floating: false,
            minimized: false,
            geometry: None,
        },
    );
}

fn ring(engine: &mut WorldEngine, output: &str, ids: &[&str]) {
    engine.world.workspace.mapping.insert(
        output.to_owned(),
        ids.iter().map(|id| (*id).to_owned()).collect(),
    );
}

fn view(engine: &mut WorldEngine, output: &str, workspace: &str) {
    engine
        .world
        .workspace
        .views
        .insert(output.to_owned(), workspace.to_owned());
}

fn native_binding(engine: &mut WorldEngine, output: &str, native: &str, workspace: &str) {
    engine
        .world
        .settlement
        .native_map
        .insert((output.to_owned(), native.to_owned()), workspace.to_owned());
}

fn detached_ref(output: &str, surface: &str) -> DetachedRef {
    DetachedRef {
        output: out(output),
        surface: crate::ids::DetachedSurfaceId::parse(surface).expect("surface id"),
    }
}

fn detached_fact(window: &str, output: &str, surface: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: Some(true),
        native_workspace: NativeWorkspaceFact::Null,
        detached_surface: Some(detached_ref(output, surface)),
        ..fact(window, output, x)
    }
}

fn native_row(output: &str, order: &[&str], current: &str) -> NativeObservation {
    NativeObservation {
        output: out(output),
        native_order: order.iter().map(|id| (*id).to_owned()).collect(),
        current_native: Some(current.to_owned()),
        occupancy: order
            .iter()
            .map(|id| NativeOccupancy {
                native: (*id).to_owned(),
                count: None,
            })
            .collect(),
        current_known: true,
        detached_current: None,
    }
}

fn detached_row(output: &str, order: &[&str], surface: &str) -> NativeObservation {
    NativeObservation {
        output: out(output),
        native_order: order.iter().map(|id| (*id).to_owned()).collect(),
        current_native: None,
        occupancy: order
            .iter()
            .map(|id| NativeOccupancy {
                native: (*id).to_owned(),
                count: None,
            })
            .collect(),
        current_known: true,
        detached_current: Some(detached_ref(output, surface)),
    }
}

fn gap_row(output: &str, order: &[&str]) -> NativeObservation {
    NativeObservation {
        output: out(output),
        native_order: order.iter().map(|id| (*id).to_owned()).collect(),
        current_native: None,
        occupancy: order
            .iter()
            .map(|id| NativeOccupancy {
                native: (*id).to_owned(),
                count: None,
            })
            .collect(),
        current_known: false,
        detached_current: None,
    }
}

fn command_report(
    seq: u64,
    screens: Vec<crate::world_report::ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
    output: &str,
    workspace: &str,
) -> WorldReportV4 {
    let mut report = report(
        seq,
        seq,
        screens,
        windows,
        Some(seq),
        Some(WorldCommand::SelectWorkspace {
            output: out(output),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        }),
    );
    report.native = native;
    report
}

fn observation_report(
    seq: u64,
    screens: Vec<crate::world_report::ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
) -> WorldReportV4 {
    let mut report = report(seq, seq, screens, windows, None, None);
    report.native = native;
    report
}

fn view_flights(engine: &WorldEngine) -> Vec<&crate::world_engine::RetainedTransfer> {
    engine
        .world
        .settlement
        .retained
        .iter()
        .filter(|retained| matches!(retained.target.kind, TransferKind::View))
        .collect()
}

fn seed_home_scene(engine: &mut WorldEngine, current: &str) {
    ring(engine, "out-a", &["ws-home", "ws-away"]);
    view(engine, "out-a", current);
    native_binding(engine, "out-a", "n-home", "ws-home");
    native_binding(engine, "out-a", "n-away", "ws-away");
    member(engine, "win-home", "out-a", "ws-home");
    member(engine, "win-fs", "out-a", "ws-away");
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-a", "ws-away"), WindowId("win-fs".to_owned()));
}

fn home_windows() -> Vec<WindowFact> {
    vec![
        fact("win-home", "out-a", 0),
        detached_fact("win-fs", "out-a", "surf-away", 100),
    ]
}

#[test]
fn ordinary_select_emits_exact_detached_destination() {
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-home");
    let reply = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 1, "one ordinary select flight: {flights:?}");
    let destination = flights[0]
        .detached_destination
        .clone()
        .expect("detached destination frozen");
    assert_eq!(destination.surface, detached_ref("out-a", "surf-away"));
    assert_eq!(destination.member, WindowId("win-fs".to_owned()));
    assert!(
        reply
            .transfers
            .iter()
            .any(|row| row.kind == TransferKind::View
                && row.detached_destination.as_ref() == Some(&destination)),
        "grant projects the frozen destination: {:?}",
        reply.transfers
    );
    assert!(
        reply.focus_request.is_none(),
        "a detached target is focusless: {:?}",
        reply.focus_request
    );
}

#[test]
fn exact_redundant_return_sends_no_view_and_no_focus() {
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-away");
    let reply = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![detached_row("out-a", &["n-home", "n-away"], "surf-away")],
            "out-a",
            "ws-away",
        ),
    );
    assert!(
        view_flights(&engine).is_empty(),
        "already exact detached current mints no View"
    );
    assert!(
        reply
            .transfers
            .iter()
            .all(|row| row.kind != TransferKind::View),
        "no view row: {:?}",
        reply.transfers
    );
    assert!(reply.focus_request.is_none(), "no focus is invented");
    assert!(
        engine.world.settlement.retained_selection.is_empty(),
        "no binding freezes for a proven redundant no-op"
    );
    let exact = engine
        .redundant_detached_selection(10)
        .expect("exact no-flight selection has a local diagnostic witness");
    assert_eq!(exact.surface, detached_ref("out-a", "surf-away"));
    assert_eq!(exact.member.0, "win-fs");
    assert!(engine.redundant_detached_selection(9).is_none());
    accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![detached_row("out-a", &["n-home", "n-away"], "surf-away")],
        ),
    );
    assert!(engine.redundant_detached_selection(10).is_none());
}

#[test]
fn same_home_gap_never_becomes_a_desktop_fallback_write() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-home", "ws-away"]);
    view(&mut engine, "out-a", "ws-away");
    native_binding(&mut engine, "out-a", "n-home", "ws-home");
    native_binding(&mut engine, "out-a", "n-away", "ws-away");
    member(&mut engine, "win-home", "out-a", "ws-away");
    let reply = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            vec![fact("win-home", "out-a", 0)],
            vec![gap_row("out-a", &["n-home", "n-away"])],
            "out-a",
            "ws-away",
        ),
    );
    assert!(
        view_flights(&engine).is_empty(),
        "same-home read gap mints no Desktop fallback"
    );
    assert!(reply.focus_request.is_none());
    assert!(engine.redundant_detached_selection(10).is_none());
}

#[test]
fn settlement_branch_a_exact_current_proves_and_gap_holds() {
    // Exact fresh detached current plus the grounded member proves the
    // focusless binding (the flight is expected to still exist at proof).
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-home");
    let _ = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let proof = accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![detached_row("out-a", &["n-home", "n-away"], "surf-away")],
        ),
    );
    assert_eq!(proof.switch_settled.len(), 1, "Branch A proves: {proof:?}");
    let event = &proof.switch_settled[0];
    assert_eq!(event.operation_id, "op-10");
    assert_eq!(event.outcome, crate::world_report::SwitchOutcome::Complete);
    assert_eq!(event.focus, crate::world_report::SwitchFocus::Unproven);
    assert!(view_flights(&engine).is_empty(), "terminal retires the flight");

    // A read gap is never a departure and never proof.
    let mut gap = detached_engine(false);
    seed_home_scene(&mut gap, "ws-home");
    let _ = accepted(
        &mut gap,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let held = accepted(
        &mut gap,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![gap_row("out-a", &["n-home", "n-away"])],
        ),
    );
    assert!(held.switch_settled.is_empty(), "gap holds");
    assert_eq!(view_flights(&gap).len(), 1, "flight survives the gap");
    let binding = gap
        .world
        .settlement
        .retained_selection
        .get(&out("out-a"))
        .expect("binding pends");
    assert!(
        binding
            .view_targets
            .get(&out("out-a"))
            .is_some_and(|target| target.is_pending()),
        "detached target stays pending"
    );

    // Wrong surface and wrong member are also not proof.
    for (window, native, label) in [
        ("win-fs", detached_row("out-a", &["n-home", "n-away"], "surf-other"), "wrong surface"),
        ("win-fs", detached_row("out-a", &["n-home", "n-away"], "surf-away"), "wrong member"),
    ] {
        let mut wrong = detached_engine(false);
        seed_home_scene(&mut wrong, "ws-home");
        let _ = accepted(
            &mut wrong,
            &command_report(
                10,
                vec![screen("out-a", 0)],
                home_windows(),
                vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
                "out-a",
                "ws-away",
            ),
        );
        let mut windows = home_windows();
        if label == "wrong surface" {
            // Current names another surface while the frozen target (and
            // its member) still name surf-away.
        }
        windows[1] = if label == "wrong member" {
            detached_fact(window, "out-a", "surf-other", 100)
        } else {
            detached_fact(window, "out-a", "surf-away", 100)
        };
        let held = accepted(
            &mut wrong,
            &observation_report(11, vec![screen("out-a", 0)], windows, vec![native]),
        );
        assert!(held.switch_settled.is_empty(), "{label} holds");
        assert_eq!(view_flights(&wrong).len(), 1, "{label} keeps the flight");
    }
}

#[test]
fn settlement_branch_b_correlated_post_and_member_proves_only_exactly() {
    let select = |engine: &mut WorldEngine| {
        seed_home_scene(engine, "ws-home");
        let _ = accepted(
            engine,
            &command_report(
                10,
                vec![screen("out-a", 0)],
                home_windows(),
                vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
                "out-a",
                "ws-away",
            ),
        );
    };
    let attempt = |post_member: Option<&str>| ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-a"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Accepted,
        pre: None,
        post: Some(ViewRef::Detached(detached_ref("out-a", "surf-away"))),
        baseline: None,
        attempt_ms: 7,
        post_member: post_member.map(|window| WindowId(window.to_owned())),
    };

    let mut engine = detached_engine(false);
    select(&mut engine);
    let mut proof = observation_report(
        11,
        vec![screen("out-a", 0)],
        home_windows(),
        // Branch A must be false here: the Desktop is current.
        vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
    );
    proof.view_attempts = vec![attempt(Some("win-fs"))];
    let reply = accepted(&mut engine, &proof);
    assert_eq!(reply.switch_settled.len(), 1, "Branch B proves: {reply:?}");
    assert_eq!(
        reply.switch_settled[0].outcome,
        crate::world_report::SwitchOutcome::Complete
    );

    // A null or different postMember withholds proof.
    for (post_member, label) in [(None, "null postMember"), (Some("win-other"), "wrong member")] {
        let mut held_engine = detached_engine(false);
        select(&mut held_engine);
        let mut held_report = observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
        );
        held_report.view_attempts = vec![attempt(post_member)];
        let reply = accepted(&mut held_engine, &held_report);
        assert!(reply.switch_settled.is_empty(), "{label} holds");
        assert_eq!(
            view_flights(&held_engine).len(),
            1,
            "{label} keeps the flight"
        );
    }
}

#[test]
fn shared_select_without_detached_capability_retains_only_the_command_output() {
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            detached_fullscreen_view: false,
            ..detached_caps()
        },
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    for output in ["out-a", "out-b"] {
        ring(&mut engine, output, &["ws-home", "ws-away"]);
        view(&mut engine, output, "ws-home");
        native_binding(&mut engine, output, "n-home", "ws-home");
        native_binding(&mut engine, output, "n-away", "ws-away");
    }
    let _ = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0), screen("out-b", 800)],
            Vec::new(),
            vec![
                native_row("out-a", &["n-home", "n-away"], "n-home"),
                native_row("out-b", &["n-home", "n-away"], "n-home"),
            ],
            "out-a",
            "ws-away",
        ),
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 1);
    assert_eq!(flights[0].target.target_output, out("out-a"));
    assert!(flights[0].detached_destination.is_none());
    let binding = engine
        .world
        .settlement
        .retained_selection
        .get(&out("out-a"))
        .expect("shared command output keeps the binding");
    assert_eq!(
        binding.view_targets.keys().cloned().collect::<Vec<_>>(),
        vec![out("out-a"), out("out-b")],
        "capability-off shared selection keeps every output's proof obligation"
    );
    for output in ["out-a", "out-b"] {
        let target = binding
            .view_targets
            .get(&out(output))
            .expect("per-output proof target");
        assert!(
            target.detached_destination.is_none(),
            "{output} keeps a Desktop proof target"
        );
    }
    for output in ["out-a", "out-b"] {
        assert_eq!(
            engine.world.workspace.views.get(output).map(String::as_str),
            Some("ws-away")
        );
    }
}

#[test]
fn shared_detached_siblings_settle_independently() {
    let mut engine = detached_engine(true);
    for output in ["out-a", "out-b"] {
        ring(&mut engine, output, &["ws-home", "ws-away"]);
        view(&mut engine, output, "ws-home");
        native_binding(&mut engine, output, "n-home", "ws-home");
        native_binding(&mut engine, output, "n-away", "ws-away");
    }
    member(&mut engine, "win-home-a", "out-a", "ws-home");
    member(&mut engine, "win-home-b", "out-b", "ws-home");
    member(&mut engine, "win-fs-a", "out-a", "ws-away");
    member(&mut engine, "win-fs-b", "out-b", "ws-away");
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-a", "ws-away"), WindowId("win-fs-a".to_owned()));
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-b", "ws-away"), WindowId("win-fs-b".to_owned()));
    let windows = || {
        let mut home_a = fact("win-home-a", "out-a", 0);
        home_a.native_workspace = NativeWorkspaceFact::Value("n-home".to_owned());
        let mut home_b = fact("win-home-b", "out-b", 800);
        home_b.native_workspace = NativeWorkspaceFact::Value("n-home".to_owned());
        vec![
            home_a,
            home_b,
            detached_fact("win-fs-a", "out-a", "surf-a", 100),
            detached_fact("win-fs-b", "out-b", "surf-b", 900),
        ]
    };
    let _ = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0), screen("out-b", 800)],
            windows(),
            vec![
                native_row("out-a", &["n-home", "n-away"], "n-home"),
                native_row("out-b", &["n-home", "n-away"], "n-home"),
            ],
            "out-a",
            "ws-away",
        ),
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 2, "one flight per planned output");
    let mut members: Vec<WindowId> = flights
        .iter()
        .filter_map(|flight| flight.detached_destination.as_ref())
        .map(|destination| destination.member.clone())
        .collect();
    members.sort();
    assert_eq!(
        members,
        vec![
            WindowId("win-fs-a".to_owned()),
            WindowId("win-fs-b".to_owned())
        ],
        "each output keeps its own intended member"
    );

    // One sibling proves; the other stays pending and the terminal waits.
    let partial = accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-a", 0), screen("out-b", 800)],
            windows(),
            vec![
                detached_row("out-a", &["n-home", "n-away"], "surf-a"),
                native_row("out-b", &["n-home", "n-away"], "n-home"),
            ],
        ),
    );
    assert!(
        partial.switch_settled.is_empty(),
        "a settled sibling never completes the shared switch alone"
    );
    let binding = engine
        .world
        .settlement
        .retained_selection
        .get(&out("out-a"))
        .expect("binding pends");
    assert_eq!(
        binding.view_targets.get(&out("out-a")).map(|t| &t.verdict),
        Some(&crate::world_selection::ViewTargetVerdict::Settled)
    );
    assert!(
        binding
            .view_targets
            .get(&out("out-b"))
            .is_some_and(|target| target.is_pending()),
        "the sibling obligation survives"
    );

    let done = accepted(
        &mut engine,
        &observation_report(
            12,
            vec![screen("out-a", 0), screen("out-b", 800)],
            windows(),
            vec![
                detached_row("out-a", &["n-home", "n-away"], "surf-a"),
                detached_row("out-b", &["n-home", "n-away"], "surf-b"),
            ],
        ),
    );
    assert_eq!(done.switch_settled.len(), 1, "both siblings prove: {done:?}");
    assert_eq!(
        done.switch_settled[0].outcome,
        crate::world_report::SwitchOutcome::Complete
    );
    assert!(view_flights(&engine).is_empty(), "all flights retire");
}

#[test]
fn lost_output_retires_only_its_target_and_rekeys_to_a_live_sibling() {
    let (mut engine, _windows) = shared_pending_selection();
    // out-a (the issuing/focus output) is truly lost; out-b survives.
    let lost = accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-b", 800)],
            vec![
                fact("win-home-b", "out-b", 800),
                detached_fact("win-fs-b", "out-b", "surf-b", 900),
            ],
            vec![native_row("out-b", &["n-home", "n-away"], "n-home")],
        ),
    );
    assert!(lost.switch_settled.is_empty(), "no terminal on the loss report");
    assert!(
        !engine
            .world
            .settlement
            .retained_selection
            .contains_key(&out("out-a")),
        "the lost issuing key is retired"
    );
    let binding = engine
        .world
        .settlement
        .retained_selection
        .get(&out("out-b"))
        .expect("effectless binding rekeys to the live pending sibling");
    assert_eq!(
        binding.view_targets.keys().cloned().collect::<Vec<_>>(),
        vec![out("out-b")],
        "only the live sibling obligation remains"
    );
    assert!(
        binding
            .view_targets
            .get(&out("out-b"))
            .is_some_and(|target| target.is_pending()),
        "the rekeyed obligation is still pending"
    );

    // The live sibling proves and the terminal publishes on it.
    let done = accepted(
        &mut engine,
        &observation_report(
            12,
            vec![screen("out-b", 800)],
            vec![
                fact("win-home-b", "out-b", 800),
                detached_fact("win-fs-b", "out-b", "surf-b", 900),
            ],
            vec![detached_row("out-b", &["n-home", "n-away"], "surf-b")],
        ),
    );
    assert_eq!(done.switch_settled.len(), 1, "rekeyed terminal: {done:?}");
    assert_eq!(done.switch_settled[0].output, out("out-b"));
    assert_eq!(
        done.switch_settled[0].outcome,
        crate::world_report::SwitchOutcome::Complete
    );
}

fn shared_pending_selection() -> (WorldEngine, Vec<WindowFact>) {
    let mut engine = detached_engine(true);
    for output in ["out-a", "out-b"] {
        ring(&mut engine, output, &["ws-home", "ws-away"]);
        view(&mut engine, output, "ws-home");
        native_binding(&mut engine, output, "n-home", "ws-home");
        native_binding(&mut engine, output, "n-away", "ws-away");
    }
    member(&mut engine, "win-home-a", "out-a", "ws-home");
    member(&mut engine, "win-home-b", "out-b", "ws-home");
    member(&mut engine, "win-fs-a", "out-a", "ws-away");
    member(&mut engine, "win-fs-b", "out-b", "ws-away");
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-a", "ws-away"), WindowId("win-fs-a".to_owned()));
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-b", "ws-away"), WindowId("win-fs-b".to_owned()));
    let mut home_a = fact("win-home-a", "out-a", 0);
    home_a.native_workspace = NativeWorkspaceFact::Value("n-home".to_owned());
    let mut home_b = fact("win-home-b", "out-b", 800);
    home_b.native_workspace = NativeWorkspaceFact::Value("n-home".to_owned());
    let windows = vec![
        home_a,
        home_b,
        detached_fact("win-fs-a", "out-a", "surf-a", 100),
        detached_fact("win-fs-b", "out-b", "surf-b", 900),
    ];
    let _ = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0), screen("out-b", 800)],
            windows.clone(),
            vec![
                native_row("out-a", &["n-home", "n-away"], "n-home"),
                native_row("out-b", &["n-home", "n-away"], "n-home"),
            ],
            "out-a",
            "ws-away",
        ),
    );
    assert_eq!(view_flights(&engine).len(), 2, "shared plan admitted both");
    (engine, windows)
}

fn seed_swap_scene(
    engine: &mut WorldEngine,
    active_view: &str,
    other_view: &str,
    detached_output: Option<&str>,
) {
    for (output, current) in [("out-a", active_view), ("out-b", other_view)] {
        ring(engine, output, &["ws-a", "ws-b"]);
        view(engine, output, current);
        native_binding(engine, output, "n-a", "ws-a");
        native_binding(engine, output, "n-b", "ws-b");
    }
    // A detached target member homed on the requested output; the plan would
    // name it if a selection path ever computed one. The planned-swap scene
    // homes it on the source output so the swap target stays unoccupied, and
    // the empty scene omits membership entirely.
    if let Some(output) = detached_output {
        member(engine, "win-fs", output, "ws-b");
        engine.world.focus.last_active_window.insert(
            key(output, "ws-b"),
            WindowId("win-fs".to_owned()),
        );
    }
}

fn swap_windows(detached_output: &str) -> Vec<WindowFact> {
    let mut win_a = fact("win-a", "out-a", 0);
    win_a.native_workspace = NativeWorkspaceFact::Value("n-a".to_owned());
    vec![
        win_a,
        detached_fact("win-fs", detached_output, "surf-swap", 100),
    ]
}

fn swap_report(
    seq: u64,
    windows: Vec<WindowFact>,
    active_native: &str,
    other_native: &str,
) -> WorldReportV4 {
    command_report(
        seq,
        vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        vec![
            native_row("out-a", &["n-a", "n-b"], active_native),
            native_row("out-b", &["n-a", "n-b"], other_native),
        ],
        "out-a",
        "ws-b",
    )
}

#[test]
fn global_unique_fallthrough_is_detached_eligible() {
    let mut engine = global_unique_engine();
    seed_swap_scene(&mut engine, "ws-a", "ws-a", Some("out-a"));
    member(&mut engine, "win-a", "out-a", "ws-a");
    let reply = accepted(&mut engine, &swap_report(11, swap_windows("out-a"), "n-a", "n-a"));
    assert_eq!(
        reply
            .transfers
            .iter()
            .filter(|row| row.kind == TransferKind::View && row.detached_destination.is_some())
            .count(),
        1,
        "ordinary non-swapping fallthrough emits the detached destination: {:?}",
        reply.transfers
    );
}

#[test]
fn global_unique_planned_swaps_never_emit_a_detached_destination() {
    // Nonempty swap: a source member moves, so the stateful branch owns the
    // view flight and must never compute a detached plan.
    let mut engine = global_unique_engine();
    seed_swap_scene(&mut engine, "ws-a", "ws-b", Some("out-b"));
    member(&mut engine, "win-a", "out-a", "ws-a");
    let swapped = accepted(&mut engine, &swap_report(11, swap_windows("out-b"), "n-a", "n-b"));
    assert_eq!(
        engine.world.workspace.views.get("out-a"),
        Some(&"ws-b".to_owned()),
        "the stateful swap moved the target view"
    );
    assert_eq!(
        engine.world.workspace.views.get("out-b"),
        Some(&"ws-a".to_owned()),
        "the stateful swap moved the prior view"
    );
    assert_eq!(view_flights(&engine).len(), 1, "the swap retains one view flight");
    assert!(
        swapped
            .transfers
            .iter()
            .filter(|row| row.kind == TransferKind::View)
            .all(|row| row.detached_destination.is_none()),
        "nonempty swap view flight carries no destination: {:?}",
        swapped.transfers
    );
    assert!(
        view_flights(&engine)
            .iter()
            .all(|flight| flight.detached_destination.is_none()),
        "retained swap view flight carries no destination"
    );

    // Empty swap: no source members/domains, still a planned swap, and the
    // mapping-only branch keeps the plain view flight.
    let mut empty = global_unique_engine();
    seed_swap_scene(&mut empty, "ws-a", "ws-b", None);
    let swapped = accepted(&mut empty, &swap_report(11, swap_windows("out-b"), "n-a", "n-b"));
    assert_eq!(
        empty.world.workspace.views.get("out-a"),
        Some(&"ws-b".to_owned()),
        "the mapping-only swap still moves the target view"
    );
    assert_eq!(
        empty.world.workspace.views.get("out-b"),
        Some(&"ws-a".to_owned()),
        "the mapping-only swap still moves the prior view"
    );
    assert_eq!(
        view_flights(&empty).len(),
        1,
        "the empty swap retains one view flight"
    );
    assert!(
        swapped
            .transfers
            .iter()
            .filter(|row| row.kind == TransferKind::View)
            .all(|row| row.detached_destination.is_none()),
        "empty swap view flight carries no destination: {:?}",
        swapped.transfers
    );
    assert!(
        view_flights(&empty)
            .iter()
            .all(|flight| flight.detached_destination.is_none()),
        "empty swap retained flight carries no destination"
    );
}

#[test]
fn physical_current_ref_is_typed_and_gaps_are_unknown() {
    let mut engine = detached_engine(false);
    let stage = |engine: &mut WorldEngine, row: NativeObservation| {
        let mut report = report(
            1,
            1,
            vec![screen("out-a", 0)],
            Vec::new(),
            None,
            None,
        );
        report.native = vec![row];
        engine.tx(|tx| tx.stage_facts_v4(&report));
    };
    stage(
        &mut engine,
        native_row("out-a", &["n-home", "n-away"], "n-home"),
    );
    assert_eq!(
        engine.tx(|tx| tx.physical_current_ref(&out("out-a"))),
        Some(ViewRef::Native {
            output: out("out-a"),
            workspace: crate::ids::NativeWorkspaceId::parse("n-home").expect("native id"),
        })
    );
    stage(
        &mut engine,
        detached_row("out-a", &["n-home", "n-away"], "surf-away"),
    );
    assert_eq!(
        engine.tx(|tx| tx.physical_current_ref(&out("out-a"))),
        Some(ViewRef::Detached(detached_ref("out-a", "surf-away")))
    );
    stage(
        &mut engine,
        gap_row("out-a", &["n-home", "n-away"]),
    );
    assert_eq!(
        engine.tx(|tx| tx.physical_current_ref(&out("out-a"))),
        None,
        "a read gap is never a typed physical view"
    );
}

#[test]
fn two_detached_members_return_to_the_remembered_member() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-home", "ws-away"]);
    view(&mut engine, "out-a", "ws-home");
    native_binding(&mut engine, "out-a", "n-home", "ws-home");
    native_binding(&mut engine, "out-a", "n-away", "ws-away");
    member(&mut engine, "win-home", "out-a", "ws-home");
    member(&mut engine, "win-fs-a", "out-a", "ws-away");
    member(&mut engine, "win-fs-b", "out-a", "ws-away");
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-a", "ws-away"), WindowId("win-fs-b".to_owned()));
    let reply = accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            vec![
                fact("win-home", "out-a", 0),
                detached_fact("win-fs-a", "out-a", "surf-a", 100),
                detached_fact("win-fs-b", "out-a", "surf-b", 200),
            ],
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 1, "one planned return: {flights:?}");
    let destination = flights[0]
        .detached_destination
        .clone()
        .expect("detached destination frozen");
    assert_eq!(
        destination.member,
        WindowId("win-fs-b".to_owned()),
        "the remembered member wins over WindowId order"
    );
    assert_eq!(destination.surface, detached_ref("out-a", "surf-b"));
    assert!(reply.focus_request.is_none(), "detached return is focusless");
}

#[test]
fn remembered_desktop_member_never_diverts_to_a_detached_member() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-home", "ws-away"]);
    view(&mut engine, "out-a", "ws-home");
    native_binding(&mut engine, "out-a", "n-home", "ws-home");
    native_binding(&mut engine, "out-a", "n-away", "ws-away");
    member(&mut engine, "win-home", "out-a", "ws-home");
    member(&mut engine, "win-desk", "out-a", "ws-away");
    member(&mut engine, "win-fs", "out-a", "ws-away");
    engine
        .world
        .focus
        .last_active_window
        .insert(key("out-a", "ws-away"), WindowId("win-desk".to_owned()));
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            vec![
                fact("win-home", "out-a", 0),
                fact("win-desk", "out-a", 100),
                detached_fact("win-fs", "out-a", "surf-away", 200),
            ],
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 1, "ordinary Desktop view flight: {flights:?}");
    assert!(
        flights.iter().all(|flight| flight.detached_destination.is_none()),
        "the remembered Desktop member never diverts to the detached sibling"
    );
}

#[test]
fn absent_remembered_focus_never_diverts_to_sorted_member() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-home", "ws-away"]);
    view(&mut engine, "out-a", "ws-home");
    native_binding(&mut engine, "out-a", "n-home", "ws-home");
    native_binding(&mut engine, "out-a", "n-away", "ws-away");
    member(&mut engine, "win-home", "out-a", "ws-home");
    member(&mut engine, "win-fs-a", "out-a", "ws-away");
    member(&mut engine, "win-fs-b", "out-a", "ws-away");
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            vec![
                fact("win-home", "out-a", 0),
                detached_fact("win-fs-a", "out-a", "surf-a", 100),
                detached_fact("win-fs-b", "out-a", "surf-b", 200),
            ],
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 1, "ordinary Desktop view flight: {flights:?}");
    assert!(
        flights.iter().all(|flight| flight.detached_destination.is_none()),
        "no remembered focus must not pick a fullscreen member by WindowId order"
    );
}

#[test]
fn ordinary_pending_desktop_selection_external_move_retires_scope_and_binding() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-1", "ws-2", "ws-3"]);
    view(&mut engine, "out-a", "ws-1");
    for (native, workspace) in [("n-1", "ws-1"), ("n-2", "ws-2"), ("n-3", "ws-3")] {
        native_binding(&mut engine, "out-a", native, workspace);
    }
    for (window, workspace) in [("win-1", "ws-1"), ("win-2", "ws-2"), ("win-3", "ws-3")] {
        member(&mut engine, window, "out-a", workspace);
    }
    let windows = || {
        vec![
            fact("win-1", "out-a", 0),
            fact("win-2", "out-a", 100),
            fact("win-3", "out-a", 200),
        ]
    };
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            windows(),
            vec![native_row("out-a", &["n-1", "n-2", "n-3"], "n-1")],
            "out-a",
            "ws-2",
        ),
    );
    assert!(
        engine
            .world
            .settlement
            .retained_selection
            .contains_key(&out("out-a")),
        "ordinary Desktop selection freezes a pending binding"
    );
    assert!(
        engine
            .world
            .settlement
            .selection_scopes
            .contains_key(&out("out-a")),
        "ordinary Desktop selection freezes a scope"
    );
    accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            windows(),
            vec![native_row("out-a", &["n-1", "n-2", "n-3"], "n-3")],
        ),
    );
    assert_eq!(
        engine.world.workspace.views.get("out-a"),
        Some(&"ws-3".to_owned()),
        "the external move is adopted"
    );
    assert!(
        engine.world.settlement.retained_selection.is_empty(),
        "the stale ordinary binding retires"
    );
    assert!(
        engine.world.settlement.selection_scopes.is_empty(),
        "the stale ordinary scope retires"
    );
}

#[test]
fn ordinary_exact_native_arrival_settles_normally_without_pre_attempt_supersession() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-1", "ws-2", "ws-3"]);
    view(&mut engine, "out-a", "ws-1");
    for (native, workspace) in [("n-1", "ws-1"), ("n-2", "ws-2"), ("n-3", "ws-3")] {
        native_binding(&mut engine, "out-a", native, workspace);
    }
    for (window, workspace) in [("win-1", "ws-1"), ("win-2", "ws-2"), ("win-3", "ws-3")] {
        member(&mut engine, window, "out-a", workspace);
    }
    let windows = || {
        vec![
            fact("win-1", "out-a", 0),
            fact("win-2", "out-a", 100),
            fact("win-3", "out-a", 200),
        ]
    };
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            windows(),
            vec![native_row("out-a", &["n-1", "n-2", "n-3"], "n-1")],
            "out-a",
            "ws-2",
        ),
    );
    let reply = accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            windows(),
            vec![native_row("out-a", &["n-1", "n-2", "n-3"], "n-2")],
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "the exact target arrival is the ordinary terminal, not external supersession: {reply:?}"
    );
    assert_eq!(
        reply.switch_settled[0].outcome,
        crate::world_report::SwitchOutcome::Complete
    );
    assert_eq!(
        engine.world.workspace.views.get("out-a"),
        Some(&"ws-2".to_owned())
    );
    assert!(view_flights(&engine).is_empty());
    assert!(engine.world.settlement.retained_selection.is_empty());
}

#[test]
fn shared_pre_attempt_supersession_keeps_a_pending_sibling_obligation() {
    let (mut engine, windows) = shared_pending_selection();
    let mut moved = observation_report(
        11,
        vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        vec![
            native_row("out-a", &["n-home", "n-away"], "n-away"),
            native_row("out-b", &["n-home", "n-away"], "n-away"),
        ],
    );
    // out-b has a correlated attempt that has not proven yet: its flight and
    // binding target must survive while out-a's external departure retires
    // only out-a.
    moved.view_attempts = vec![ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-b"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Accepted,
        pre: Some(ViewRef::Native {
            output: out("out-b"),
            workspace: crate::ids::NativeWorkspaceId::parse("n-home").expect("native id"),
        }),
        post: Some(ViewRef::Native {
            output: out("out-b"),
            workspace: crate::ids::NativeWorkspaceId::parse("n-away").expect("native id"),
        }),
        baseline: None,
        attempt_ms: 7,
        post_member: None,
    }];
    let reply = accepted(&mut engine, &moved);
    assert!(
        reply.switch_settled.is_empty(),
        "the pending sibling keeps the shared terminal open: {reply:?}"
    );
    let flights = view_flights(&engine);
    assert_eq!(flights.len(), 1, "only the pending sibling flight survives");
    assert_eq!(flights[0].target.target_output, out("out-b"));
    assert!(flights[0].detached_destination.is_some());
    let binding = engine
        .world
        .settlement
        .retained_selection
        .get(&out("out-a"))
        .expect("the superseded output's binding key still carries its sibling");
    assert_eq!(
        binding.view_targets.keys().cloned().collect::<Vec<_>>(),
        vec![out("out-b")],
        "out-a's obligation is retired per-output, out-b's is preserved"
    );
    assert!(
        binding
            .view_targets
            .get(&out("out-b"))
            .is_some_and(|target| target.is_pending()),
        "the sibling's pending proof is untouched"
    );
    assert!(
        engine
            .world
            .settlement
            .selection_scopes
            .contains_key(&out("out-b")),
        "the sibling scope survives"
    );
    assert!(
        !engine
            .world
            .settlement
            .selection_scopes
            .contains_key(&out("out-a")),
        "only the superseded output's scope retires"
    );
    for output in ["out-a", "out-b"] {
        assert_eq!(
            engine.world.workspace.views.get(output),
            Some(&"ws-away".to_owned()),
            "the coherent shared departure is adopted for {output}"
        );
    }
}

#[test]
fn detached_pre_attempt_external_desktop_departure_supersedes_without_replay() {
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-home");
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    assert_eq!(view_flights(&engine).len(), 1, "detached flight admitted");
    let moved = accepted(
        &mut engine,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-away")],
        ),
    );
    assert!(
        moved.switch_settled.is_empty(),
        "external supersession mints no event: {moved:?}"
    );
    assert_eq!(
        engine.world.workspace.views.get("out-a"),
        Some(&"ws-away".to_owned()),
        "the observed Desktop logical is adopted once"
    );
    assert!(
        view_flights(&engine).is_empty(),
        "the superseded detached flight retires"
    );
    assert!(
        engine.world.settlement.retained_selection.is_empty(),
        "the matching binding retires"
    );
    assert!(
        engine.world.settlement.selection_scopes.is_empty(),
        "the matching scope retires"
    );
    assert!(
        engine
            .world
            .settlement
            .retained
            .iter()
            .all(|flight| flight.target.operation_id != "op-10"),
        "no replay or sibling flight survives"
    );
}

#[test]
fn detached_pre_attempt_supersession_holds_on_gap_preimage_and_attempt() {
    let select = |engine: &mut WorldEngine| {
        seed_home_scene(engine, "ws-home");
        accepted(
            engine,
            &command_report(
                10,
                vec![screen("out-a", 0)],
                home_windows(),
                vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
                "out-a",
                "ws-away",
            ),
        );
    };
    // A read gap is never a departure.
    let mut gap = detached_engine(false);
    select(&mut gap);
    accepted(
        &mut gap,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![gap_row("out-a", &["n-home", "n-away"])],
        ),
    );
    assert_eq!(view_flights(&gap).len(), 1, "gap holds the detached flight");
    assert!(
        gap.world
            .settlement
            .retained_selection
            .contains_key(&out("out-a")),
        "gap keeps the binding"
    );
    // An unchanged frozen preimage is not newer external input.
    let mut unchanged = detached_engine(false);
    select(&mut unchanged);
    accepted(
        &mut unchanged,
        &observation_report(
            11,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
        ),
    );
    assert_eq!(
        view_flights(&unchanged).len(),
        1,
        "unchanged preimage holds the detached flight"
    );
    assert!(
        unchanged
            .world
            .settlement
            .retained_selection
            .contains_key(&out("out-a")),
        "unchanged preimage keeps the binding"
    );
    // A correlated attempt owns the proof lanes; the external pre-attempt
    // supersession never races it.
    let mut attempted = detached_engine(false);
    select(&mut attempted);
    let mut with_attempt = observation_report(
        11,
        vec![screen("out-a", 0)],
        home_windows(),
        vec![native_row("out-a", &["n-home", "n-away"], "n-away")],
    );
    with_attempt.view_attempts = vec![ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-a"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Refused,
        pre: Some(ViewRef::Native {
            output: out("out-a"),
            workspace: crate::ids::NativeWorkspaceId::parse("n-away").expect("native id"),
        }),
        post: Some(ViewRef::Native {
            output: out("out-a"),
            workspace: crate::ids::NativeWorkspaceId::parse("n-away").expect("native id"),
        }),
        baseline: None,
        attempt_ms: 7,
        post_member: None,
    }];
    accepted(&mut attempted, &with_attempt);
    assert_eq!(
        view_flights(&attempted).len(),
        1,
        "a correlated attempt holds the flight for the proof lanes"
    );
    assert!(
        attempted
            .world
            .settlement
            .retained_selection
            .contains_key(&out("out-a")),
        "the attempt keeps the binding pending"
    );
}

#[test]
fn branch_b_terminal_adopts_same_reply_desktop_logical() {
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-home");
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let mut proof = observation_report(
        11,
        vec![screen("out-a", 0)],
        home_windows(),
        vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
    );
    proof.view_attempts = vec![ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-a"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Accepted,
        pre: None,
        post: Some(ViewRef::Detached(detached_ref("out-a", "surf-away"))),
        baseline: None,
        attempt_ms: 7,
        post_member: Some(WindowId("win-fs".to_owned())),
    }];
    let reply = accepted(&mut engine, &proof);
    assert_eq!(reply.switch_settled.len(), 1, "Branch B proves: {reply:?}");
    assert_eq!(
        engine.world.workspace.views.get("out-a"),
        Some(&"ws-home".to_owned()),
        "the same reply adopts the newer coherent Desktop instead of echoing ws-away"
    );
}

#[test]
fn true_detached_refusal_terminal_adopts_same_reply_desktop_logical() {
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-home");
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let mut refused = observation_report(
        11,
        vec![screen("out-a", 0)],
        home_windows(),
        vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
    );
    let preimage = ViewRef::Native {
        output: out("out-a"),
        workspace: crate::ids::NativeWorkspaceId::parse("n-home").expect("native id"),
    };
    refused.view_attempts = vec![ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-a"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Refused,
        pre: Some(preimage.clone()),
        post: Some(preimage),
        baseline: None,
        attempt_ms: 7,
        post_member: None,
    }];
    let reply = accepted(&mut engine, &refused);
    assert_eq!(reply.switch_settled.len(), 1, "refusal terminal: {reply:?}");
    assert_eq!(
        reply.switch_settled[0].outcome,
        crate::world_report::SwitchOutcome::CompletedWithFailures
    );
    assert_eq!(
        engine.world.workspace.views.get("out-a"),
        Some(&"ws-home".to_owned()),
        "the refusal adopts the unchanged readable Desktop preimage"
    );
}

#[test]
fn detached_refusal_holds_under_the_gesture_barrier() {
    let mut engine = detached_engine(false);
    seed_home_scene(&mut engine, "ws-home");
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            home_windows(),
            vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
            "out-a",
            "ws-away",
        ),
    );
    let preimage = ViewRef::Native {
        output: out("out-a"),
        workspace: crate::ids::NativeWorkspaceId::parse("n-home").expect("native id"),
    };
    let mut refused = observation_report(
        11,
        vec![screen("out-a", 0)],
        home_windows(),
        vec![native_row("out-a", &["n-home", "n-away"], "n-home")],
    );
    refused.gesture_barrier = true;
    refused.view_attempts = vec![ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-a"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Refused,
        pre: Some(preimage.clone()),
        post: Some(preimage),
        baseline: None,
        attempt_ms: 7,
        post_member: None,
    }];
    let reply = accepted(&mut engine, &refused);
    assert!(
        reply.switch_settled.is_empty(),
        "gesture barrier holds the detached refusal: {reply:?}"
    );
    assert_eq!(
        view_flights(&engine).len(),
        1,
        "the detached flight survives the barrier"
    );
}

#[test]
fn detached_preimage_refusal_holds_under_the_gesture_barrier() {
    let mut engine = detached_engine(false);
    ring(&mut engine, "out-a", &["ws-home", "ws-away"]);
    view(&mut engine, "out-a", "ws-home");
    native_binding(&mut engine, "out-a", "n-away", "ws-away");
    member(&mut engine, "win-fs", "out-a", "ws-home");
    member(&mut engine, "win-away", "out-a", "ws-away");
    let windows = || {
        vec![
            detached_fact("win-fs", "out-a", "surf-home", 0),
            fact("win-away", "out-a", 100),
        ]
    };
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0)],
            windows(),
            vec![detached_row("out-a", &["n-away"], "surf-home")],
            "out-a",
            "ws-away",
        ),
    );
    let preimage = ViewRef::Detached(detached_ref("out-a", "surf-home"));
    let mut refused = observation_report(
        11,
        vec![screen("out-a", 0)],
        windows(),
        vec![detached_row("out-a", &["n-away"], "surf-home")],
    );
    refused.gesture_barrier = true;
    refused.view_attempts = vec![ViewAttempt {
        operation_id: "op-10".to_owned(),
        target_output: out("out-a"),
        target_workspace: "ws-away".to_owned(),
        disposition: FrameSetter::Refused,
        pre: Some(preimage.clone()),
        post: Some(preimage),
        baseline: None,
        attempt_ms: 7,
        post_member: None,
    }];
    let reply = accepted(&mut engine, &refused);
    assert!(
        reply.switch_settled.is_empty(),
        "gesture barrier holds the detached-preimage refusal: {reply:?}"
    );
    assert_eq!(
        view_flights(&engine).len(),
        1,
        "the Desktop obligation survives the barrier"
    );
}

#[test]
fn shared_exact_current_sibling_keeps_no_cursor_guard() {
    let mut engine = detached_engine(true);
    for output in ["out-a", "out-b"] {
        ring(&mut engine, output, &["ws-home", "ws-away"]);
        view(&mut engine, output, "ws-home");
        native_binding(&mut engine, output, "n-home", "ws-home");
        native_binding(&mut engine, output, "n-away", "ws-away");
    }
    member(&mut engine, "win-home-a", "out-a", "ws-home");
    member(&mut engine, "win-home-b", "out-b", "ws-home");
    member(&mut engine, "win-fs-a", "out-a", "ws-away");
    member(&mut engine, "win-fs-b", "out-b", "ws-away");
    for (output, window) in [("out-a", "win-fs-a"), ("out-b", "win-fs-b")] {
        engine
            .world
            .focus
            .last_active_window
            .insert(key(output, "ws-away"), WindowId(window.to_owned()));
    }
    accepted(
        &mut engine,
        &command_report(
            10,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![
                fact("win-home-a", "out-a", 0),
                fact("win-home-b", "out-b", 800),
                detached_fact("win-fs-a", "out-a", "surf-a", 100),
                detached_fact("win-fs-b", "out-b", "surf-b", 900),
            ],
            vec![
                detached_row("out-a", &["n-home", "n-away"], "surf-a"),
                native_row("out-b", &["n-home", "n-away"], "n-home"),
            ],
            "out-a",
            "ws-away",
        ),
    );
    assert_eq!(
        view_flights(&engine).len(),
        1,
        "only the non-exact sibling dispatches"
    );
    assert!(
        !engine
            .world
            .focus
            .detached_cursor_guards
            .contains_key(&out("out-a")),
        "an already exact-current sibling keeps no new guard"
    );
    assert!(
        engine
            .world
            .focus
            .detached_cursor_guards
            .contains_key(&out("out-b")),
        "the admitted sibling owns the retained guard"
    );
}
