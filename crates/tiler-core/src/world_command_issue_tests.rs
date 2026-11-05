//! Dual-intake command-path parity (all-4c increment): the same helper
//! outcomes on the legacy capture and the v4 capture, with raw inputs set
//! on the source reports BEFORE intake and retained joins seeded
//! separately. The contra reports below deliberately disagree on
//! focus/views/sequences so only staged inputs can explain the outcomes.
//!
//! Scope: helper-level parity plus one legacy full-path envelope check.
//! Nothing here proves v4 runtime/scenario behavior (v4 has no accepted-run
//! assembly yet); the v4 legs stage through the real v4 capture and assert
//! the same helper results as the legacy legs. Settlement-lane proof rows
//! (arrival/view echoes) are intentionally absent: those lanes are owned
//! by the settle proofs, not the command helpers tested here.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::CoreCommand;
use crate::connection::HostCapabilities;
use crate::directional::{OutputId, SameAxisMove, WindowId, WorkspaceId};
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::workspace_scope::WorkspaceMode;
use crate::world_commands::CommandApplied;
use crate::world_directional::tests::{accepted, fact, rect, report_full, screen};
use crate::world_engine::{
    EventIssue, KIND_FOCUS, KIND_MIGRATION, KIND_OPERATION, KIND_SCOPE_HELD, KIND_SEND_WINDOW,
    MSG_MIGRATION_INVENTORY, MSG_MIGRATION_SHARED, RawRouting, StagedDomain, WorldEngine,
    WorldOutcome,
};
use crate::world_members::MemberState;
use crate::world_report::{
    EffectCause, PresentationIngressV4, PresentationRowV4, SendSource, StateProperty,
    WORLD_CONTRACT_VERSION_V4, WorldCommand, WorldReport, WorldReportV4,
};

fn new_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: true,
            maximize: true,
            fullscreen: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn new_composed_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: true,
            maximize: false,
            fullscreen: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn test_issue(report: &WorldReport) -> EventIssue {
    EventIssue {
        owner: crate::ids::OwnerId::parse("adapter-1").expect("owner"),
        correlation: crate::ids::CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

fn home_key_ws(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn report_v4_simple(seq: u64, screens: Vec<crate::world_report::ScreenReport>) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens,
        held_screens: Vec::new(),
        windows: Vec::new(),
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

/// Independently built same-content pair (no conversion): raw lane fields
/// are set on each member BEFORE staging; `stage_either` only picks the
/// intake.
fn pair_focus_reports() -> (WorldReport, WorldReportV4) {
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    let legacy = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        windows.clone(),
        vec![("out-a", "ws-1")],
        "win-a",
        None,
        None,
    );
    let mut v4 = report_v4_simple(2, vec![screen("out-a", 0)]);
    v4.windows = windows;
    v4.focused_global = "win-a".to_owned();
    (legacy, v4)
}

fn stage_either(
    engine: &mut WorldEngine,
    v4: bool,
    legacy: &WorldReport,
    v4report: &WorldReportV4,
) {
    if v4 {
        let observation = WorldEngine::capture_observation_v4(v4report);
        engine.ingest_staged_intent(&observation);
        engine.stage_facts_v4(v4report);
    } else {
        let observation = WorldEngine::capture_observation(legacy);
        engine.ingest_staged_intent(&observation);
        engine.stage_facts(legacy);
    }
}

/// Retained ring/views join plus one readable domain for `window`: stands
/// in for prior accepted-run assembly; classification lanes still come
/// from the real staged captures above.
fn seed_ring_views(engine: &mut WorldEngine) {
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
    engine
        .last_views
        .insert(OutputId("out-a".to_owned()), "ws-1".to_owned());
}

/// Retained command-planning domain join (intake-independent): the staged
/// domain index the command helpers plan over, mirroring accepted-run
/// assembly. Seeded AFTER staging, which rebuilds the staged report.
fn seed_command_domain(engine: &mut WorldEngine) {
    let key = home_key_ws("out-a", "ws-1");
    let win = |id: &str, x: i32| EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: rect(x, 0, 40, 90),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    };
    engine
        .staged
        .homing
        .insert(WindowId("win-a".to_owned()), key.clone());
    engine
        .staged
        .homing
        .insert(WindowId("win-b".to_owned()), key.clone());
    engine.staged.domains.push(StagedDomain {
        key,
        bounds: rect(0, 0, 800, 560),
        gap: 0,
        outer_gap: 0,
        windows: vec![win("win-a", 0), win("win-b", 200)],
        focused: WindowId("win-a".to_owned()),
        minimized: Vec::new(),
        unreadable: Vec::new(),
        closed: Vec::new(),
        readable: true,
        hold: None,
    });
}

/// Contra report for command callees: same screens, no command,
/// deliberately wrong focus/views/sequences so only staged inputs plus
/// the explicit issue can explain an outcome.
fn contra_report(seq: u64) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        vec![("out-a", "ws-9")],
        "win-other",
        None,
        None,
    )
}

fn focus_command(window: &str) -> WorldCommand {
    WorldCommand::Focus {
        window: WindowId(window.to_owned()),
        direction: "left".to_owned(),
    }
}

#[test]
fn focus_command_effect_parity_with_unknown_refusal() {
    for v4 in [false, true] {
        let mut engine = new_engine();
        seed_ring_views(&mut engine);
        let (legacy, v4report) = pair_focus_reports();
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_command_domain(&mut engine);
        // Eligible control: known staged window effects with the envelope
        // sequence carried through.
        let probe = contra_report(90);
        let issue = test_issue(&probe);
        match engine.execute_command(
            &probe,
            &issue,
            12,
            &focus_command("win-b"),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Effect(effect) => {
                assert_eq!(effect.target, WindowId("win-b".to_owned()));
                assert_eq!(effect.command_seq, Some(12));
                assert_eq!(effect.cause, EffectCause::ExplicitCommand);
            }
            CommandApplied::Rejected { kind, message } => {
                panic!("eligible focus must effect, got {kind} {message} (v4={v4})")
            }
            CommandApplied::NoEffect => panic!("eligible focus must effect (v4={v4})"),
        }
        // Negative with the eligible control above: unknown lifetimes
        // refuse with the focus vocabulary on both intakes.
        match engine.execute_command(
            &probe,
            &issue,
            13,
            &focus_command("win-zzz"),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_FOCUS),
            _ => panic!("unknown focus must refuse (v4={v4})"),
        }
    }
}

#[test]
fn sticky_transition_uses_issue_owner_with_ownerless_connection_and_invalid_rid() {
    // Event-owner authorization: the eligible native sticky transition
    // authorizes from the explicit issue alone. The connection owner is
    // absent and the passed report carries an invalid request id, so any
    // connection/rereport-derived owner read would refuse or fail to
    // parse; success plus the minted sticky binding proves issue ownership.
    for v4 in [false, true] {
        let mut engine = new_engine();
        let windows = vec![fact("win-a", "out-a", 0)];
        let bootstrap = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            windows.clone(),
            vec![("out-a", "ws-1")],
            "win-a",
            None,
            None,
        );
        accepted(&mut engine, &bootstrap);
        let mut v4report = report_v4_simple(1, vec![screen("out-a", 0)]);
        v4report.windows = windows.clone();
        v4report.focused_global = "win-a".to_owned();
        stage_either(&mut engine, v4, &bootstrap, &v4report);
        // Destination floats so the eligible ON transition converges
        // without an engine session; the sweep then runs under the issue
        // owner with the restaged facts.
        let dest = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(
                engine
                    .effective_view(&OutputId("out-a".to_owned()))
                    .expect("staged view"),
            ),
        };
        engine.floating_scopes.insert(dest.clone());
        engine.connection_owner = None;
        // Staged intent-plus-facts staging does not rebuild the command
        // planning joins; seed the retained homing/domain join post
        // staging, mirroring accepted-run assembly.
        seed_command_domain(&mut engine);
        let mut probe = report_full(
            90,
            90,
            vec![screen("out-a", 0)],
            windows,
            vec![("out-a", "ws-1")],
            "win-other",
            None,
            None,
        );
        probe.request_id = "!!!invalid".to_owned();
        let issue = EventIssue {
            owner: OwnerId::parse("adapter-1").expect("owner"),
            correlation: CorrelationId::parse("req-7").expect("correlation"),
        };
        match engine.execute_sticky_command(
            &probe,
            &issue,
            7,
            &WindowId("win-a".to_owned()),
            Some(true),
            &BTreeSet::new(),
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => panic!(
                "eligible sticky ON must succeed with ownerless connection, got {kind} {message} (v4={v4})"
            ),
            CommandApplied::Effect(_) => panic!("sticky ON mints no focus effect (v4={v4})"),
        }
        assert_eq!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
                .map(|binding| binding.target.value),
            Some(true),
            "native sticky binding minted from the issue owner (v4={v4})",
        );
        assert!(
            engine.connection_owner.is_none(),
            "helper must not reconnect the owner (v4={v4})"
        );
    }
}

#[test]
fn local_toggle_orientation_uses_issue_owner_with_ownerless_connection_and_invalid_rid() {
    // Event-owner authorization for the event-constructing local route:
    // an eligible staged orientation flip succeeds with an ownerless
    // connection and an invalid passed request id, while a wrong-owner
    // issue refuses via engine/session authorization without mutating.
    for v4 in [false, true] {
        for wrong_owner in [false, true] {
            let mut engine = new_engine();
            let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
            let bootstrap = report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                windows.clone(),
                vec![("out-a", "ws-1")],
                "win-a",
                None,
                None,
            );
            accepted(&mut engine, &bootstrap);
            let mut v4report = report_v4_simple(1, vec![screen("out-a", 0)]);
            v4report.windows = windows.clone();
            v4report.focused_global = "win-a".to_owned();
            stage_either(&mut engine, v4, &bootstrap, &v4report);
            engine.connection_owner = None;
            let mut probe = report_full(
                90,
                90,
                vec![screen("out-a", 0)],
                windows,
                vec![("out-a", "ws-1")],
                "win-a",
                None,
                None,
            );
            probe.request_id = "!!!invalid".to_owned();
            // Real domain join over the staged facts (classification
            // only; the invalid request id never validates here),
            // mirroring accepted-run assembly.
            engine.build_staged_domains();
            let issue = EventIssue {
                owner: OwnerId::parse(if wrong_owner { "intruder" } else { "adapter-1" })
                    .expect("owner"),
                correlation: CorrelationId::parse("req-7").expect("correlation"),
            };
            let mut owned: Vec<EngineWindow> = Vec::new();
            for domain in &engine.staged.domains {
                owned.extend(domain.windows.iter().cloned());
            }
            let by_id: BTreeMap<&WindowId, &EngineWindow> =
                owned.iter().map(|row| (&row.window, row)).collect();
            let command = CoreCommand::ToggleOrientation {
                window: "win-a".to_owned(),
            };
            let revision_before = engine.state_revision;
            match engine.run_local_operation(&probe, &issue, 7, &command, &by_id, &BTreeSet::new())
            {
                CommandApplied::NoEffect => {
                    assert!(
                        !wrong_owner,
                        "wrong-owner orientation must not succeed (v4={v4})"
                    );
                    assert_eq!(
                        engine.command_highwater,
                        Some(7),
                        "eligible orientation consumes its sequence through the real event path (v4={v4})",
                        v4 = v4,
                    );
                }
                CommandApplied::Rejected { kind, message } => assert!(
                    wrong_owner,
                    "eligible orientation must succeed with ownerless connection, got refusal {kind} {message} (v4={v4})"
                ),
                CommandApplied::Effect(_) => panic!("orientation emits no focus effect (v4={v4})"),
            }
            if wrong_owner {
                assert_eq!(
                    engine.state_revision, revision_before,
                    "wrong-owner refusal mutates nothing (v4={v4})"
                );
                assert!(
                    engine.command_highwater.is_none(),
                    "wrong-owner refusal consumes no sequence (v4={v4})"
                );
            }
        }
    }
}

#[test]
fn toggle_maximize_focus_gate_reads_staged_not_contra() {
    // Staged maximize focus gate vs the contra report: the staged
    // observation (not the passed focus/views) decides, on both intakes.
    for v4 in [false, true] {
        // Eligible: staged focuses win-a over a readable homing while the
        // contra report focuses elsewhere with stale views.
        let mut engine = new_engine();
        seed_ring_views(&mut engine);
        let (legacy, v4report) = pair_focus_reports();
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_command_domain(&mut engine);
        let probe = contra_report(90);
        match engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &WorldCommand::ToggleMaximize {
                window: WindowId("win-a".to_owned()),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => {
                panic!("eligible maximize must toggle, refused {kind} {message} (v4={v4})")
            }
            CommandApplied::Effect(_) => panic!("maximize mints no focus effect (v4={v4})"),
        }
        assert!(
            matches!(
                engine.command_result,
                Some(crate::world_report::CommandResult::Maximize {
                    maximized: true,
                    ..
                })
            ),
            "native maximize result minted from staged facts (v4={v4}): {:?}",
            engine.command_result,
            v4 = v4,
        );
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
                .is_some_and(|binding| binding.target.value),
            "maximized binding retained (v4={v4})",
            v4 = v4,
        );
        // Negative with the eligible contra above: staged focuses an
        // unknown window, so the gate refuses even though the passed
        // report focuses the eligible one.
        let mut refused_engine = new_engine();
        seed_ring_views(&mut refused_engine);
        let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
        let refused_legacy = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.clone(),
            vec![("out-a", "ws-1")],
            "win-zzz",
            None,
            None,
        );
        let mut refused_v4 = report_v4_simple(2, vec![screen("out-a", 0)]);
        refused_v4.windows = windows;
        refused_v4.focused_global = "win-zzz".to_owned();
        stage_either(&mut refused_engine, v4, &refused_legacy, &refused_v4);
        seed_command_domain(&mut refused_engine);
        let mut eligible_probe = contra_report(91);
        eligible_probe.focused_global = "win-a".to_owned();
        match refused_engine.execute_command(
            &eligible_probe,
            &test_issue(&eligible_probe),
            12,
            &WorldCommand::ToggleMaximize {
                window: WindowId("win-a".to_owned()),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_FOCUS),
            CommandApplied::NoEffect => panic!("staged unknown focus must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
        // Negative with staged eligibility intact: a held home scope
        // refuses; the contra report carries no held information.
        let mut held_engine = new_engine();
        seed_ring_views(&mut held_engine);
        let (legacy, v4report) = pair_focus_reports();
        stage_either(&mut held_engine, v4, &legacy, &v4report);
        seed_command_domain(&mut held_engine);
        let mut held = BTreeSet::new();
        held.insert(home_key_ws("out-a", "ws-1"));
        let held_probe = contra_report(92);
        match held_engine.execute_command(
            &held_probe,
            &test_issue(&held_probe),
            12,
            &WorldCommand::ToggleMaximize {
                window: WindowId("win-a".to_owned()),
            },
            &held,
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_SCOPE_HELD),
            CommandApplied::NoEffect => panic!("held home scope must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
    }
}

#[test]
fn composed_maximize_staged_inventory_and_barrier_gates() {
    // Core-composed maximize (`caps.maximize == false`) reads the staged
    // observation for frame-write availability: incomplete staged
    // inventory and a staged gesture barrier each refuse on their own
    // gate, while the eligible press toggles the owned bit. Raw
    // `maximized` stays null on the source reports (a non-null native
    // claim fails closed) and both raw flags are set before intake; the
    // contra probe always claims a clean lane.
    let stage_pair = |inventory_complete: bool, gesture_barrier: bool| {
        let mut win = fact("win-a", "out-a", 0);
        win.maximized = None;
        let windows = vec![win];
        let mut legacy = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.clone(),
            vec![("out-a", "ws-1")],
            "win-a",
            None,
            None,
        );
        legacy.inventory_complete = inventory_complete;
        legacy.gesture_barrier = gesture_barrier;
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0)]);
        v4report.windows = windows;
        v4report.focused_global = "win-a".to_owned();
        v4report.inventory_complete = inventory_complete;
        v4report.gesture_barrier = gesture_barrier;
        (legacy, v4report)
    };
    for v4 in [false, true] {
        // Eligible: staged focus plus a readable homed domain while the
        // contra report focuses elsewhere, so only staged state explains
        // the owned bit.
        let mut engine = new_composed_engine();
        seed_ring_views(&mut engine);
        let (legacy, v4report) = stage_pair(true, false);
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_command_domain(&mut engine);
        let probe = contra_report(90);
        match engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &WorldCommand::ToggleMaximize {
                window: WindowId("win-a".to_owned()),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => {
                panic!("eligible composed maximize must toggle, refused {kind} {message} (v4={v4})")
            }
            CommandApplied::Effect(_) => panic!("maximize mints no focus effect (v4={v4})"),
        }
        match &engine.command_result {
            Some(crate::world_report::CommandResult::Maximize { maximized, .. }) => assert!(
                *maximized,
                "owned bit set from staged facts (v4={v4})",
                v4 = v4,
            ),
            other => panic!("expected maximize result, got {other:?} (v4={v4})"),
        }
        assert!(
            engine
                .composed_maximized
                .contains(&WindowId("win-a".to_owned())),
            "composed bit retained (v4={v4})",
            v4 = v4,
        );
        // Negative with the eligible control above: incomplete staged
        // inventory refuses even though the contra probe claims a
        // complete one.
        let mut partial_engine = new_composed_engine();
        seed_ring_views(&mut partial_engine);
        let (legacy, v4report) = stage_pair(false, false);
        stage_either(&mut partial_engine, v4, &legacy, &v4report);
        seed_command_domain(&mut partial_engine);
        let mut complete_probe = contra_report(91);
        complete_probe.inventory_complete = true;
        match partial_engine.execute_command(
            &complete_probe,
            &test_issue(&complete_probe),
            12,
            &WorldCommand::ToggleMaximize {
                window: WindowId("win-a".to_owned()),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_OPERATION),
            CommandApplied::NoEffect => panic!("partial staged inventory must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
        assert!(
            !partial_engine
                .composed_maximized
                .contains(&WindowId("win-a".to_owned())),
            "inventory refusal sets no bit (v4={v4})",
            v4 = v4,
        );
        // Negative with the eligible control above: a staged gesture
        // barrier refuses even though the contra probe claims a clean
        // lane.
        let mut barrier_engine = new_composed_engine();
        seed_ring_views(&mut barrier_engine);
        let (legacy, v4report) = stage_pair(true, true);
        stage_either(&mut barrier_engine, v4, &legacy, &v4report);
        seed_command_domain(&mut barrier_engine);
        let barrier_probe = contra_report(92);
        match barrier_engine.execute_command(
            &barrier_probe,
            &test_issue(&barrier_probe),
            12,
            &WorldCommand::ToggleMaximize {
                window: WindowId("win-a".to_owned()),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_OPERATION),
            CommandApplied::NoEffect => panic!("staged barrier must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
        assert!(
            !barrier_engine
                .composed_maximized
                .contains(&WindowId("win-a".to_owned())),
            "barrier refusal sets no bit (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn move_float_destination_reads_staged_screens_not_contra() {
    // Staged move float-destination screens geometry: the half-snap rect
    // derives from the staged screen work area, never the contra report's
    // default geometry; a staged topology missing the home output refuses
    // even when the contra report carries that screen.
    for v4 in [false, true] {
        let custom = crate::world_report::ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(rect(100, 0, 600, 600)),
            work_area: Some(rect(100, 0, 600, 500)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        };
        let windows = vec![fact("win-a", "out-a", 0)];
        let legacy = report_full(
            2,
            2,
            vec![custom.clone()],
            windows.clone(),
            vec![("out-a", "ws-1")],
            "win-a",
            None,
            None,
        );
        let mut v4report = report_v4_simple(2, vec![custom]);
        v4report.windows = windows;
        v4report.focused_global = "win-a".to_owned();
        let mut engine = new_engine();
        seed_ring_views(&mut engine);
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_float_domain(&mut engine);
        // The contra report carries the default work area on the same
        // output: only the staged custom geometry can explain the rect.
        let work = rect(100, 0, 600, 500);
        let contra_default = rect(0, 0, 400, 560);
        let probe = contra_report(90);
        match engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
                same_axis: SameAxisMove::GroupWithNeighbor,
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => {
                panic!("eligible float move must apply, refused {kind} {message} (v4={v4})")
            }
            CommandApplied::Effect(_) => panic!("float move mints no focus effect (v4={v4})"),
        }
        match &engine.command_result {
            Some(crate::world_report::CommandResult::Directional {
                outcome,
                target,
                rect,
                ..
            }) => {
                assert_eq!(
                    *outcome,
                    crate::world_directional::DirectionalOutcome::MoveFloatApplied,
                    "float half-snap applied (v4={v4})",
                    v4 = v4,
                );
                assert_eq!(*target, Some(WindowId("win-a".to_owned())));
                let applied = rect.as_ref().expect("half-snap rect");
                assert!(
                    applied.x >= work.x
                        && applied.y >= work.y
                        && applied.x + applied.w <= work.x + work.w
                        && applied.y + applied.h <= work.y + work.h,
                    "rect {applied:?} stays inside the staged custom work area (v4={v4})",
                    v4 = v4,
                );
                assert!(
                    *applied != contra_default,
                    "rect {applied:?} must not match the contra default geometry (v4={v4})",
                    v4 = v4,
                );
            }
            other => panic!("expected directional result, got {other:?} (v4={v4})"),
        }
        // Negative: staged screens lack the home output while the contra
        // report carries it, so the move refuses on staged topology.
        let mut missing_engine = new_engine();
        seed_ring_views(&mut missing_engine);
        let off_legacy = report_full(
            2,
            2,
            vec![screen("out-b", 800)],
            vec![fact("win-a", "out-a", 0)],
            vec![("out-a", "ws-1")],
            "win-a",
            None,
            None,
        );
        let mut off_v4 = report_v4_simple(2, vec![screen("out-b", 800)]);
        off_v4.windows = vec![fact("win-a", "out-a", 0)];
        off_v4.focused_global = "win-a".to_owned();
        stage_either(&mut missing_engine, v4, &off_legacy, &off_v4);
        seed_float_domain(&mut missing_engine);
        let mut home_probe = contra_report(91);
        home_probe.screens = vec![screen("out-a", 0)];
        match missing_engine.execute_command(
            &home_probe,
            &test_issue(&home_probe),
            12,
            &WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
                same_axis: SameAxisMove::GroupWithNeighbor,
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => {
                panic!("missing staged screen must no-effect, refused {kind} {message} (v4={v4})")
            }
            CommandApplied::Effect(_) => panic!("refusal-path mints no focus effect (v4={v4})"),
        }
        match &missing_engine.command_result {
            Some(crate::world_report::CommandResult::Directional { outcome, .. }) => assert_eq!(
                *outcome,
                crate::world_directional::DirectionalOutcome::MoveFloatRefusedUnknownOutput,
                "staged topology refusal despite contra screen (v4={v4})",
                v4 = v4,
            ),
            other => panic!("expected directional refusal, got {other:?} (v4={v4})"),
        }
    }
}

/// Retained command-planning float domain (intake-independent): one
/// floating staged carrier homed on out-a/ws-1 with a readable domain,
/// mirroring accepted-run assembly for the float half-snap legs.
fn seed_float_domain(engine: &mut WorldEngine) {
    let key = home_key_ws("out-a", "ws-1");
    engine
        .staged
        .homing
        .insert(WindowId("win-a".to_owned()), key.clone());
    engine.staged.domains.push(StagedDomain {
        key,
        bounds: rect(100, 0, 600, 600),
        gap: 0,
        outer_gap: 0,
        windows: vec![EngineWindow {
            window: WindowId("win-a".to_owned()),
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            rect: rect(300, 100, 200, 200),
            floating: true,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        }],
        focused: WindowId("win-a".to_owned()),
        minimized: Vec::new(),
        unreadable: Vec::new(),
        closed: Vec::new(),
        readable: true,
        hold: None,
    });
}

#[test]
fn shared_retain_view_fans_out_across_staged_screens_not_contra() {
    // Shared mode fans the retained view out to every live staged screen;
    // the single-screen contra report cannot explain the second output's
    // move. The per-output control keeps the other output untouched.
    for v4 in [false, true] {
        for (mode, fanout) in [
            (WorkspaceMode::Shared, true),
            (WorkspaceMode::PerOutputLocal, false),
        ] {
            let mut engine = new_engine();
            engine.settings.workspace_mode = mode;
            for out in ["out-a", "out-b"] {
                engine
                    .mapping
                    .insert(out.to_owned(), vec!["ws-1".to_owned(), "ws-9".to_owned()]);
                engine.views.insert(out.to_owned(), "ws-1".to_owned());
            }
            let windows = vec![fact("win-a", "out-a", 0)];
            let legacy = report_full(
                2,
                2,
                vec![screen("out-a", 0), screen("out-b", 800)],
                windows.clone(),
                vec![("out-a", "ws-1"), ("out-b", "ws-1")],
                "win-a",
                None,
                None,
            );
            let mut v4report = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
            v4report.windows = windows;
            v4report.focused_global = "win-a".to_owned();
            stage_either(&mut engine, v4, &legacy, &v4report);
            seed_command_domain(&mut engine);
            let probe = contra_report(90);
            match engine.execute_command(
                &probe,
                &test_issue(&probe),
                12,
                &WorldCommand::SwitchView {
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-9".to_owned(),
                },
                &BTreeSet::new(),
                None,
                None,
            ) {
                CommandApplied::NoEffect => {}
                CommandApplied::Rejected { kind, message } => {
                    panic!("eligible view switch must move, refused {kind} {message} (v4={v4})")
                }
                CommandApplied::Effect(_) => panic!("view switch mints no focus effect (v4={v4})"),
            }
            assert_eq!(
                engine.views.get("out-a"),
                Some(&"ws-9".to_owned()),
                "target view moves (mode={mode:?}, v4={v4})",
                v4 = v4,
            );
            assert_eq!(
                engine.views.get("out-b"),
                Some(&(if fanout { "ws-9" } else { "ws-1" }).to_owned()),
                "second output follows only under Shared fanout (mode={mode:?}, v4={v4})",
                v4 = v4,
            );
        }
    }
}

#[test]
fn global_unique_select_swap_gated_by_staged_inventory_not_contra() {
    // GlobalUnique select swaps the views visible on two outputs. One
    // retained member forces the stateful swap path where the staged
    // inventory gate lives; the swap then refuses on staged incomplete
    // inventory even though the contra report claims a complete one.
    // (The empty-topology fast path carries no inventory gate by design.)
    let stage_pair = |inventory_complete: bool| {
        let windows = vec![fact("win-a", "out-a", 0)];
        let mut legacy = report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            windows.clone(),
            vec![("out-a", "ws-1"), ("out-b", "ws-9")],
            "win-a",
            None,
            None,
        );
        legacy.inventory_complete = inventory_complete;
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
        v4report.windows = windows;
        v4report.focused_global = "win-a".to_owned();
        v4report.inventory_complete = inventory_complete;
        (legacy, v4report)
    };
    for v4 in [false, true] {
        // Eligible swap: ws-9 visible on out-b moves to out-a and ws-1
        // returns to out-b. One retained member forces the stateful swap
        // path where the staged inventory gate lives (the empty-topology
        // fast path carries no inventory gate by design).
        let mut engine = new_engine();
        engine.settings.workspace_mode = WorkspaceMode::GlobalUnique;
        for out in ["out-a", "out-b"] {
            engine
                .mapping
                .insert(out.to_owned(), vec!["ws-1".to_owned(), "ws-9".to_owned()]);
        }
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.views.insert("out-b".to_owned(), "ws-9".to_owned());
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key_ws("out-a", "ws-1"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (legacy, v4report) = stage_pair(true);
        stage_either(&mut engine, v4, &legacy, &v4report);
        let probe = contra_report(90);
        let applied = engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-9".to_owned(),
            },
            &BTreeSet::new(),
            None,
            None,
        );
        match applied {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => {
                panic!("eligible swap must move, refused {kind} {message} (v4={v4})")
            }
            CommandApplied::Effect(_) => panic!("eligible swap invents no focus (v4={v4})"),
        }
        assert_eq!(
            engine.views.get("out-a"),
            Some(&"ws-9".to_owned()),
            "select lands on the target (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            engine.views.get("out-b"),
            Some(&"ws-1".to_owned()),
            "other output takes the displaced view (v4={v4})",
            v4 = v4,
        );
        // Negative: staged incomplete inventory refuses with the migration
        // vocabulary while the contra report claims completeness.
        let mut held_engine = new_engine();
        held_engine.settings.workspace_mode = WorkspaceMode::GlobalUnique;
        for out in ["out-a", "out-b"] {
            held_engine
                .mapping
                .insert(out.to_owned(), vec!["ws-1".to_owned(), "ws-9".to_owned()]);
        }
        held_engine
            .views
            .insert("out-a".to_owned(), "ws-1".to_owned());
        held_engine
            .views
            .insert("out-b".to_owned(), "ws-9".to_owned());
        held_engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key_ws("out-a", "ws-1"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (legacy, v4report) = stage_pair(false);
        stage_either(&mut held_engine, v4, &legacy, &v4report);
        let mut complete_probe = contra_report(91);
        complete_probe.inventory_complete = true;
        match held_engine.execute_command(
            &complete_probe,
            &test_issue(&complete_probe),
            12,
            &WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-9".to_owned(),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_MIGRATION),
            CommandApplied::NoEffect => panic!("incomplete staged inventory must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
        assert_eq!(
            held_engine.views.get("out-a"),
            Some(&"ws-1".to_owned()),
            "refusal leaves views untouched (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn migrate_mode_and_inventory_gates_read_staged_not_contra() {
    // Whole-workspace migration gates discriminate staged state against
    // the contra report: the eligible floating carry commits on both
    // intakes, Shared mode refuses before any other gate, and incomplete
    // staged inventory refuses even when the contra report claims
    // completeness. (This helper reads no gesture barrier; the composed
    // maximize lane above pins that gate separately.)
    let stage_pair = |inventory_complete: bool| {
        let windows = vec![fact("win-a", "out-a", 0)];
        let mut legacy = report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            windows.clone(),
            vec![("out-a", "ws-1"), ("out-b", "ws-9")],
            "win-a",
            None,
            None,
        );
        legacy.inventory_complete = inventory_complete;
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
        v4report.windows = windows;
        v4report.focused_global = "win-a".to_owned();
        v4report.inventory_complete = inventory_complete;
        (legacy, v4report)
    };
    // Retained joins for the floating carry, seeded after staging like
    // the other command domains: rings on both outputs, live two-screen
    // topology, current views, plus the readable staged source domain
    // with its floating scope. Identical for every leg; only the raw
    // inventory flag (set before intake) and the engine mode vary.
    let seed_migrate_joins = |engine: &mut WorldEngine| {
        engine.settings.workspace_mode = WorkspaceMode::PerOutputLocal;
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine
            .mapping
            .insert("out-b".to_owned(), vec!["ws-9".to_owned()]);
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.views.insert("out-b".to_owned(), "ws-9".to_owned());
        engine
            .live_topology
            .insert("out-a".to_owned(), rect(0, 0, 800, 600));
        engine
            .live_topology
            .insert("out-b".to_owned(), rect(800, 0, 800, 600));
        seed_float_domain(engine);
        engine.floating_scopes.insert(home_key_ws("out-a", "ws-1"));
    };
    let migrate = || WorldCommand::MigrateWorkspace {
        source_output: OutputId("out-a".to_owned()),
        target_output: OutputId("out-b".to_owned()),
        workspace: "ws-1".to_owned(),
    };
    for v4 in [false, true] {
        // Eligible: the floating source workspace carries to out-b through
        // the sessionless membership remap. The contra probe carries one
        // default screen, so the staged topology alone explains the move.
        let mut engine = new_engine();
        let (legacy, v4report) = stage_pair(true);
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_migrate_joins(&mut engine);
        let probe = contra_report(90);
        match engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &migrate(),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => {
                panic!("eligible migration must carry, refused {kind} {message} (v4={v4})")
            }
            CommandApplied::Effect(_) => panic!("migration mints no focus effect (v4={v4})"),
        }
        match &engine.command_result {
            Some(crate::world_report::CommandResult::Migration {
                source,
                target,
                members,
                ..
            }) => {
                assert_eq!(source.output, OutputId("out-a".to_owned()));
                assert_eq!(target.output, OutputId("out-b".to_owned()));
                assert!(
                    members.contains(&WindowId("win-a".to_owned())),
                    "staged source member carried (v4={v4}): {members:?}",
                    v4 = v4,
                );
            }
            other => panic!("expected migration provenance, got {other:?} (v4={v4})"),
        }
        assert_eq!(
            engine.views.get("out-b"),
            Some(&"ws-1".to_owned()),
            "target view lands (v4={v4})",
            v4 = v4,
        );
        assert!(
            !engine
                .mapping
                .get("out-a")
                .is_some_and(|ring| ring.iter().any(|id| id == "ws-1")),
            "source ring drops the id (v4={v4}): {:?}",
            engine.mapping.get("out-a"),
            v4 = v4,
        );
        assert!(
            engine.retained.iter().any(|transfer| {
                matches!(
                    &transfer.target.kind,
                    crate::world_report::TransferKind::Migration
                ) && transfer.target.operation_id == "op-12"
            }),
            "migration flight retained (v4={v4})",
            v4 = v4,
        );
        // Negative with the eligible setup above: Shared mode refuses
        // with its own vocabulary on both intakes.
        let mut shared_engine = new_engine();
        let (legacy, v4report) = stage_pair(true);
        stage_either(&mut shared_engine, v4, &legacy, &v4report);
        seed_migrate_joins(&mut shared_engine);
        shared_engine.settings.workspace_mode = WorkspaceMode::Shared;
        let shared_probe = contra_report(90);
        match shared_engine.execute_command(
            &shared_probe,
            &test_issue(&shared_probe),
            12,
            &migrate(),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, message } => {
                assert_eq!(kind, KIND_MIGRATION);
                assert_eq!(message, MSG_MIGRATION_SHARED);
            }
            CommandApplied::NoEffect => panic!("shared migration must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
        // Negative with the eligible setup above: incomplete staged
        // inventory refuses while the contra report claims completeness.
        let mut partial_engine = new_engine();
        let (legacy, v4report) = stage_pair(false);
        stage_either(&mut partial_engine, v4, &legacy, &v4report);
        seed_migrate_joins(&mut partial_engine);
        let mut complete_probe = contra_report(91);
        complete_probe.inventory_complete = true;
        match partial_engine.execute_command(
            &complete_probe,
            &test_issue(&complete_probe),
            12,
            &migrate(),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, message } => {
                assert_eq!(kind, KIND_MIGRATION);
                assert_eq!(message, MSG_MIGRATION_INVENTORY);
            }
            CommandApplied::NoEffect => panic!("partial staged inventory must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
        assert_eq!(
            partial_engine.views.get("out-b"),
            Some(&"ws-9".to_owned()),
            "refusal leaves views untouched (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn send_source_freeze_prefers_retained_home_over_current_action() {
    // The implicit-send freeze reads the source report context, never
    // stale staged state: retained homing wins over the current action
    // context, while the current action/cursor still resolves from the
    // source (not the stale staged chain). Source-freeze survival through
    // staging end to end is covered by the accepted-run freeze tests.
    for v4 in [false, true] {
        let mut engine = new_engine();
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key_ws("out-a", "ws-1"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        // Stale staged chain: no foreground, no cursor, cached out-a, and
        // an ordered registry naming only out-a.
        let mut stale_legacy = report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fact("win-a", "out-a", 0)],
            vec![("out-a", "ws-1")],
            "",
            None,
            None,
        );
        stale_legacy.action_foreground = None;
        stale_legacy.action_cached = Some(OutputId("out-a".to_owned()));
        stale_legacy.action_outputs = vec![OutputId("out-a".to_owned())];
        let mut stale_v4 = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
        stale_v4.windows = vec![fact("win-a", "out-a", 0)];
        stale_v4.action_cached = Some(OutputId("out-a".to_owned()));
        stale_v4.action_outputs = vec![OutputId("out-a".to_owned())];
        stage_either(&mut engine, v4, &stale_legacy, &stale_v4);
        // Current source context: foreground and cursor both on out-b
        // with a registry ordering out-b first.
        let mut current = report_full(
            3,
            3,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fact("win-a", "out-a", 0)],
            vec![("out-a", "ws-1")],
            "",
            None,
            None,
        );
        current.action_foreground = Some(OutputId("out-b".to_owned()));
        current.cursor = Some((850, 10));
        current.action_outputs = vec![OutputId("out-b".to_owned()), OutputId("out-a".to_owned())];
        let source_ctx = RawRouting::from_source(&current);
        let staged_ctx = RawRouting::from_staged(&engine.staged);
        assert_eq!(
            engine.resolve_home_then_action(&source_ctx, &WindowId("win-a".to_owned())),
            Some(OutputId("out-a".to_owned())),
            "retained home wins over current action (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            engine.resolve_action_output(&source_ctx),
            Some(OutputId("out-b".to_owned())),
            "current action resolves from the source context (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            engine.resolve_action_output(&staged_ctx),
            Some(OutputId("out-a".to_owned())),
            "stale staged chain disagrees, proving the freeze must use the source (v4={v4})",
            v4 = v4,
        );
        // Unfrozen helper plumbing refuses without inventing a source;
        // the frozen source resolves verbatim.
        let probe = contra_report(90);
        match engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                source: SendSource::HomeThenAction,
                index: 1,
                follow: false,
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_SEND_WINDOW),
            CommandApplied::NoEffect => panic!("unfrozen implicit source must refuse (v4={v4})"),
            CommandApplied::Effect(_) => panic!("refusal mints no effect (v4={v4})"),
        }
    }
}

#[test]
fn fresh_focus_current_retained_native_and_primary_order() {
    for v4 in [false, true] {
        // Retained join: win-a homed on out-a. Staged facts carry win-a
        // plus a brand-new win-new on out-b; live screens on three
        // outputs with the designated primary (out-c) kept clear of the
        // focus legs, so each basis resolves to its own output.
        let setup = |focused: &str| {
            let windows = vec![fact("win-a", "out-a", 0), fact("win-new", "out-b", 800)];
            let legacy = report_full(
                3,
                3,
                vec![
                    screen("out-a", 0),
                    screen("out-b", 800),
                    screen("out-c", 1600),
                ],
                windows.clone(),
                Vec::new(),
                focused,
                None,
                None,
            );
            let mut v4report = report_v4_simple(
                3,
                vec![
                    screen("out-a", 0),
                    screen("out-b", 800),
                    screen("out-c", 1600),
                ],
            );
            v4report.windows = windows;
            v4report.focused_global = focused.to_owned();
            v4report.primary_output = Some(OutputId("out-c".to_owned()));
            (legacy, v4report)
        };
        // Current managed focus wins over a competing retained latch
        // while it names retained homing.
        let mut engine = new_engine();
        seed_ring_views(&mut engine);
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key_ws("out-a", "ws-1"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        engine.fresh_focus.last_output = Some(OutputId("out-b".to_owned()));
        let (legacy, v4report) = setup("win-a");
        let mut legacy_focused = legacy.clone();
        legacy_focused.primary_output = Some(OutputId("out-c".to_owned()));
        stage_either(&mut engine, v4, &legacy_focused, &v4report);
        let (current, trace) =
            engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
        assert_eq!(
            current,
            Some(OutputId("out-a".to_owned())),
            "current wins (v4={v4})",
            v4 = v4
        );
        assert_eq!(
            trace.basis,
            crate::fresh_focus::FreshFocusBasis::Current,
            "current basis (v4={v4})",
            v4 = v4,
        );
        // Empty focus with a live retained latch falls back to retained.
        let mut engine = new_engine();
        seed_ring_views(&mut engine);
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key_ws("out-a", "ws-1"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        engine.fresh_focus.last_output = Some(OutputId("out-b".to_owned()));
        let (legacy, v4report) = setup("");
        let mut legacy_empty = legacy.clone();
        legacy_empty.primary_output = Some(OutputId("out-c".to_owned()));
        stage_either(&mut engine, v4, &legacy_empty, &v4report);
        let (retained, trace) =
            engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
        assert_eq!(
            retained,
            Some(OutputId("out-b".to_owned())),
            "retained wins (v4={v4})",
            v4 = v4
        );
        assert_eq!(
            trace.basis,
            crate::fresh_focus::FreshFocusBasis::Retained,
            "retained basis (v4={v4})",
            v4 = v4,
        );
        // No latch: the brand-new native fact wins over primary, on a
        // different output than the designated primary below.
        let mut engine = new_engine();
        let (legacy, v4report) = setup("win-new");
        let mut legacy_new = legacy.clone();
        legacy_new.primary_output = Some(OutputId("out-c".to_owned()));
        stage_either(&mut engine, v4, &legacy_new, &v4report);
        let (native, trace) = engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
        assert_eq!(
            native,
            Some(OutputId("out-b".to_owned())),
            "native wins (v4={v4})",
            v4 = v4
        );
        assert_eq!(
            trace.basis,
            crate::fresh_focus::FreshFocusBasis::Native,
            "native basis (v4={v4})",
            v4 = v4,
        );
        let ordered = engine.primary_first_outputs(&RawRouting::from_staged(&engine.staged));
        assert_eq!(
            ordered.first(),
            Some(&OutputId("out-c".to_owned())),
            "primary chain disagrees with the native pick (v4={v4})",
            v4 = v4,
        );
        // Nothing observed: no fresh focus; the primary-first chain still
        // names the designated output first.
        let mut engine = new_engine();
        let (legacy, v4report) = setup("");
        let mut legacy_none = legacy.clone();
        legacy_none.primary_output = Some(OutputId("out-c".to_owned()));
        stage_either(&mut engine, v4, &legacy_none, &v4report);
        let (none, trace) = engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
        assert_eq!(none, None, "no invention (v4={v4})", v4 = v4);
        assert_eq!(
            trace.basis,
            crate::fresh_focus::FreshFocusBasis::Primary,
            "primary basis (v4={v4})",
            v4 = v4,
        );
        let ordered = engine.primary_first_outputs(&RawRouting::from_staged(&engine.staged));
        assert_eq!(
            ordered.first(),
            Some(&OutputId("out-c".to_owned())),
            "designated primary first (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn switch_view_command_moves_view_parity() {
    // View selection through the migrated command helper on the default
    // local mode: the staged ring and screens authorize the move on both
    // intakes, with no invented focus.
    for v4 in [false, true] {
        let mut engine = new_engine();
        seed_ring_views(&mut engine);
        let windows = vec![fact("win-a", "out-a", 0)];
        let legacy = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.clone(),
            vec![("out-a", "ws-1")],
            "win-a",
            None,
            None,
        );
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0)]);
        v4report.windows = windows;
        v4report.focused_global = "win-a".to_owned();
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_command_domain(&mut engine);
        let probe = contra_report(90);
        let command = WorldCommand::SwitchView {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-9".to_owned(),
        };
        let applied = engine.execute_command(
            &probe,
            &test_issue(&probe),
            12,
            &command,
            &BTreeSet::new(),
            None,
            None,
        );
        assert!(
            matches!(applied, CommandApplied::NoEffect),
            "empty target view moves with no invented focus (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            engine.views.get("out-a"),
            Some(&"ws-9".to_owned()),
            "view moved on both intakes (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn handle_report_echoes_envelope_request_and_command_sequence() {
    // Full legacy path: the reply echoes the envelope request id and the
    // one-shot effect carries the dispatching command sequence; an
    // observation-only report stays accepted with no effects and no
    // unconditional failure.
    let mut engine = new_engine();
    let bootstrap = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
        vec![("out-a", "ws-1")],
        "win-a",
        None,
        None,
    );
    match engine.handle_report(&bootstrap) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("bootstrap must accept, got {other:?}"),
    }
    let command_report = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
        vec![("out-a", "ws-1")],
        "win-a",
        Some(7),
        Some(focus_command("win-b")),
    );
    let reply = match engine.handle_report(&command_report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("command report must accept, got {other:?}"),
    };
    assert_eq!(reply.request_id, "req-2");
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target, WindowId("win-b".to_owned()));
    assert_eq!(reply.effects[0].command_seq, Some(7));
    assert_eq!(reply.effects[0].cause, EffectCause::ExplicitCommand);
    let observation_only = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 200)],
        vec![("out-a", "ws-1")],
        "win-a",
        None,
        None,
    );
    let reply = match engine.handle_report(&observation_only) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("observation-only must accept, got {other:?}"),
    };
    assert_eq!(reply.request_id, "req-3");
    assert!(
        reply.effects.is_empty(),
        "observation-only emits no effects: {:?}",
        reply.effects
    );
}
