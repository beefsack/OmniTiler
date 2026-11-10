//! Command-helper issue-ownership proofs (v4-only): the same explicit
//! outcomes staged through the real v4 capture, with raw inputs set on
//! the source v4 reports BEFORE intake and retained joins seeded
//! separately. The contra v4 reports below deliberately disagree on
//! focus/sequences so only staged inputs can explain the outcomes.
//!
//! Scope: helper-level assertions through self-sufficient direct v4
//! entry. Nothing here proves v4 runtime/scenario behavior beyond the
//! staged command helpers; settlement-lane proof rows (arrival/view
//! echoes) are intentionally absent: those lanes are owned by the settle
//! proofs, not the command helpers tested here.

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
use crate::world_engine::{
    EventIssue, KIND_FOCUS, KIND_MIGRATION, KIND_OPERATION, KIND_SCOPE_HELD, KIND_SEND_WINDOW,
    MSG_MIGRATION_INVENTORY, MSG_MIGRATION_SHARED, RawRouting, StagedDomain, WorldEngine,
    WorldOutcome,
};
use crate::world_members::MemberState;
use crate::world_report::{
    EffectCause, PresentationIngressV4, PresentationRowV4, SendSource, StateProperty,
    WORLD_CONTRACT_VERSION_V4, WorldCommand, WorldReportV4,
};
use crate::world_v4_support_tests::{fact, rect, screen};

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

fn test_issue(report: &WorldReportV4) -> EventIssue {
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
        foreground_known: false,
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

/// Independently built v4 content (no conversion): raw lane fields are
/// set on the member BEFORE staging.
fn focus_report_v4() -> WorldReportV4 {
    let mut v4 = report_v4_simple(2, vec![screen("out-a", 0)]);
    v4.windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    v4.focused_global = "win-a".to_owned();
    v4.foreground_known = true;
    v4
}

fn stage_v4(engine: &mut WorldEngine, report: &WorldReportV4) {
    let observation = WorldEngine::capture_observation_v4(report);
    engine.ingest_staged_intent(&observation);
    engine.stage_facts_v4(report);
}

fn accepted_v4(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
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

/// Contra v4 report for command callees: same screens, no command,
/// deliberately wrong focus/sequences so only staged inputs plus
/// the explicit issue can explain an outcome.
fn contra_v4(seq: u64) -> WorldReportV4 {
    let mut report = report_v4_simple(seq, vec![screen("out-a", 0)]);
    report.windows = vec![fact("win-a", "out-a", 0)];
    report.focused_global = "win-other".to_owned();
    report.foreground_known = true;
    report
}

fn focus_command(window: &str) -> WorldCommand {
    WorldCommand::Focus {
        window: WindowId(window.to_owned()),
        direction: "left".to_owned(),
    }
}

#[test]
fn focus_command_effect_parity_with_unknown_refusal() {
    let mut engine = new_engine();
    seed_ring_views(&mut engine);
    let v4report = focus_report_v4();
    stage_v4(&mut engine, &v4report);
    seed_command_domain(&mut engine);
    // Eligible control: known staged window effects with the envelope
    // sequence carried through.
    let probe = contra_v4(90);
    let issue = test_issue(&probe);
    match engine.execute_command(
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
            panic!("eligible focus must effect, got {kind} {message}")
        }
        CommandApplied::NoEffect => panic!("eligible focus must effect"),
    }
    // Negative with the eligible control above: unknown lifetimes
    // refuse with the focus vocabulary.
    match engine.execute_command(
        &issue,
        13,
        &focus_command("win-zzz"),
        &BTreeSet::new(),
        None,
        None,
    ) {
        CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_FOCUS),
        _ => panic!("unknown focus must refuse"),
    }
}

#[test]
fn sticky_transition_uses_issue_owner_with_ownerless_connection() {
    // Event-owner authorization: the eligible native sticky transition
    // authorizes from the explicit issue alone with no report passed at
    // all. The connection owner is absent, so any connection-derived
    // owner read would refuse; success plus the minted sticky binding
    // proves issue ownership.
    let mut engine = new_engine();
    let windows = vec![fact("win-a", "out-a", 0)];
    let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
    bootstrap.windows = windows.clone();
    bootstrap.focused_global = "win-a".to_owned();
    bootstrap.foreground_known = true;
    accepted_v4(&mut engine, &bootstrap);
    stage_v4(&mut engine, &bootstrap);
    seed_ring_views(&mut engine);
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
    let issue = EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse("req-7").expect("correlation"),
    };
    match engine.execute_sticky_command(
        &issue,
        7,
        &WindowId("win-a".to_owned()),
        Some(true),
        &BTreeSet::new(),
    ) {
        CommandApplied::NoEffect => {}
        CommandApplied::Rejected { kind, message } => panic!(
            "eligible sticky ON must succeed with ownerless connection, got {kind} {message}"
        ),
        CommandApplied::Effect(_) => panic!("sticky ON mints no focus effect"),
    }
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .map(|binding| binding.target.value),
        Some(true),
        "native sticky binding minted from the issue owner",
    );
    assert!(
        engine.connection_owner.is_none(),
        "helper must not reconnect the owner"
    );
}

/// Toggle-float clear restage guard: the composed clear clones the engine,
/// drops sticky provenance, and reclassifies from staged state with no
/// report input at all. The staged restage converges and adopts the
/// tile-base save.
#[test]
fn sticky_toggle_float_clear_restages_staged_facts_only() {
    use crate::intent_data::IntentData;

    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            ..Default::default()
        },
    )
    .expect("generation");
    // Null sticky keeps the row live without the native capability;
    // restored provenance below is what makes the clear eligible.
    let mut staged_row = fact("win-a", "out-a", 0);
    staged_row.sticky = None;
    let windows = vec![staged_row];
    let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
    bootstrap.windows = windows.clone();
    bootstrap.focused_global = "win-a".to_owned();
    bootstrap.foreground_known = true;
    accepted_v4(&mut engine, &bootstrap);
    stage_v4(&mut engine, &bootstrap);
    seed_ring_views(&mut engine);
    let dest = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(
            engine
                .effective_view(&OutputId("out-a".to_owned()))
                .expect("staged view"),
        ),
    };
    engine.floating_scopes.insert(dest.clone());
    seed_command_domain(&mut engine);
    let win = WindowId("win-a".to_owned());
    engine.restored_sticky.insert(win.clone(), false);
    engine
        .pending_intent
        .insert(win.clone(), IntentData::new(None, None));
    // No report is ever passed to the helper: the explicit issue is
    // the only identity input, staged state the only content input.
    let issue = EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse("req-7").expect("correlation"),
    };
    match engine.execute_sticky_toggle_float(&issue, 7, &win, &BTreeSet::new()) {
        CommandApplied::NoEffect => {}
        CommandApplied::Rejected { kind, message } => panic!(
            "staged toggle-float clear must succeed with no report input, got {kind} {message}"
        ),
        CommandApplied::Effect(_) => {
            panic!("toggle-float clear mints no focus effect")
        }
    }
    assert!(
        !engine.restored_sticky.contains_key(&win),
        "clear drops sticky provenance",
    );
    assert_eq!(
        engine.sticky_saved.get(&win),
        Some(&IntentData::new(Some(false), None)),
        "tile-base save adopted from staged facts",
    );
    assert!(
        !engine.pending_intent.contains_key(&win),
        "restaged adopt must not reingest cleared intent",
    );
    assert_eq!(
        engine.staged_homing(&win).as_ref(),
        Some(&dest),
        "restage converges at the destination",
    );
}

/// Staged-only restage guard: the composed sticky transition clones the
/// engine, mutates the intent ledgers, and reclassifies from staged state
/// with no report input at all. The staged restage converges and adopts
/// the ON save.
#[test]
fn sticky_composed_transition_restages_staged_facts_only() {
    use crate::intent_data::IntentData;

    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            ..Default::default()
        },
    )
    .expect("generation");
    // Projected sticky values grade to held without the native
    // capability, so the staged row carries null (same shape as the
    // composed-route suites); the transition still records its save.
    let mut staged_row = fact("win-a", "out-a", 0);
    staged_row.sticky = None;
    let windows = vec![staged_row];
    let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
    bootstrap.windows = windows.clone();
    bootstrap.focused_global = "win-a".to_owned();
    bootstrap.foreground_known = true;
    accepted_v4(&mut engine, &bootstrap);
    stage_v4(&mut engine, &bootstrap);
    seed_ring_views(&mut engine);
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
    // Staged intent-plus-facts staging does not rebuild the command
    // planning joins; seed the retained homing/domain join post
    // staging, mirroring accepted-run assembly.
    seed_command_domain(&mut engine);
    // A command-cleared import: the transition drops pending on its
    // clone, and the restaged adopt must not resurrect it.
    let win = WindowId("win-a".to_owned());
    engine
        .pending_intent
        .insert(win.clone(), IntentData::new(None, None));
    // No report is ever passed to the helper: the explicit issue is
    // the only identity input, staged state the only content input.
    let issue = EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse("req-7").expect("correlation"),
    };
    match engine.execute_sticky_command(&issue, 7, &win, Some(true), &BTreeSet::new()) {
        CommandApplied::NoEffect => {}
        CommandApplied::Rejected { kind, message } => {
            panic!("staged composed ON must succeed with no report input, got {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("sticky ON mints no focus effect"),
    }
    assert_eq!(
        engine.restored_sticky.get(&win),
        Some(&false),
        "tiled origin restores with a false prior",
    );
    assert_eq!(
        engine.sticky_saved.get(&win),
        Some(&IntentData::new(Some(true), Some(false))),
        "ON save adopted from staged facts",
    );
    assert!(
        engine.sticky_visible_rev.contains_key(&win),
        "fresh ON marks the visibility revision",
    );
    assert!(
        !engine.pending_intent.contains_key(&win),
        "restaged adopt must not reingest cleared intent",
    );
}

#[test]
fn local_toggle_orientation_uses_issue_owner_with_ownerless_connection() {
    // Event-owner authorization for the event-constructing local route:
    // an eligible staged orientation flip succeeds with an ownerless
    // connection and no report passed at all, while a wrong-owner
    // issue refuses via engine/session authorization without mutating.
    for wrong_owner in [false, true] {
        let mut engine = new_engine();
        let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
        let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
        bootstrap.windows = windows.clone();
        bootstrap.focused_global = "win-a".to_owned();
        bootstrap.foreground_known = true;
        accepted_v4(&mut engine, &bootstrap);
        stage_v4(&mut engine, &bootstrap);
        seed_ring_views(&mut engine);
        engine.connection_owner = None;
        // Real domain join over the staged facts (classification
        // only, from staged state with no report input), mirroring
        // accepted-run assembly.
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
        match engine.run_local_operation(&issue, 7, &command, &by_id, &BTreeSet::new()) {
            CommandApplied::NoEffect => {
                assert!(!wrong_owner, "wrong-owner orientation must not succeed");
                assert_eq!(
                    engine.command_highwater,
                    Some(7),
                    "eligible orientation consumes its sequence through the real event path",
                );
            }
            CommandApplied::Rejected { kind, message } => assert!(
                wrong_owner,
                "eligible orientation must succeed with ownerless connection, got refusal {kind} {message}"
            ),
            CommandApplied::Effect(_) => panic!("orientation emits no focus effect"),
        }
        if wrong_owner {
            assert_eq!(
                engine.state_revision, revision_before,
                "wrong-owner refusal mutates nothing"
            );
            assert!(
                engine.command_highwater.is_none(),
                "wrong-owner refusal consumes no sequence"
            );
        }
    }
}

#[test]
fn toggle_maximize_focus_gate_reads_staged_not_contra() {
    // Staged maximize focus gate vs the contra report: the staged
    // observation (not the passed focus) decides.
    // Eligible: staged focuses win-a over a readable homing while the
    // contra report focuses elsewhere.
    let mut engine = new_engine();
    seed_ring_views(&mut engine);
    let v4report = focus_report_v4();
    stage_v4(&mut engine, &v4report);
    seed_command_domain(&mut engine);
    let probe = contra_v4(90);
    match engine.execute_command(
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
            panic!("eligible maximize must toggle, refused {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("maximize mints no focus effect"),
    }
    assert!(
        matches!(
            engine.command_result,
            Some(crate::world_report::CommandResult::Maximize {
                maximized: true,
                ..
            })
        ),
        "native maximize result minted from staged facts: {:?}",
        engine.command_result,
    );
    assert!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
            .is_some_and(|binding| binding.target.value),
        "maximized binding retained",
    );
    // Negative with the eligible contra above: staged focuses an
    // unknown window, so the gate refuses even though the passed
    // report focuses the eligible one.
    let mut refused_engine = new_engine();
    seed_ring_views(&mut refused_engine);
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    let mut refused_v4 = report_v4_simple(2, vec![screen("out-a", 0)]);
    refused_v4.windows = windows;
    refused_v4.focused_global = "win-zzz".to_owned();
    refused_v4.foreground_known = true;
    stage_v4(&mut refused_engine, &refused_v4);
    seed_command_domain(&mut refused_engine);
    let mut eligible_probe = contra_v4(91);
    eligible_probe.focused_global = "win-a".to_owned();
    eligible_probe.foreground_known = true;
    match refused_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("staged unknown focus must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
    // Negative with staged eligibility intact: a held home scope
    // refuses; the contra report carries no held information.
    let mut held_engine = new_engine();
    seed_ring_views(&mut held_engine);
    let v4report = focus_report_v4();
    stage_v4(&mut held_engine, &v4report);
    seed_command_domain(&mut held_engine);
    let mut held = BTreeSet::new();
    held.insert(home_key_ws("out-a", "ws-1"));
    let held_probe = contra_v4(92);
    match held_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("held home scope must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
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
    let stage_v4report = |inventory_complete: bool, gesture_barrier: bool| {
        let mut win = fact("win-a", "out-a", 0);
        win.maximized = None;
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0)]);
        v4report.windows = vec![win];
        v4report.focused_global = "win-a".to_owned();
        v4report.foreground_known = true;
        v4report.inventory_complete = inventory_complete;
        v4report.gesture_barrier = gesture_barrier;
        v4report
    };
    // Eligible: staged focus plus a readable homed domain while the
    // contra report focuses elsewhere, so only staged state explains
    // the owned bit.
    let mut engine = new_composed_engine();
    seed_ring_views(&mut engine);
    let v4report = stage_v4report(true, false);
    stage_v4(&mut engine, &v4report);
    seed_command_domain(&mut engine);
    let probe = contra_v4(90);
    match engine.execute_command(
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
            panic!("eligible composed maximize must toggle, refused {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("maximize mints no focus effect"),
    }
    match &engine.command_result {
        Some(crate::world_report::CommandResult::Maximize { maximized, .. }) => {
            assert!(*maximized, "owned bit set from staged facts",)
        }
        other => panic!("expected maximize result, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "composed bit retained",
    );
    // Negative with the eligible control above: incomplete staged
    // inventory refuses even though the contra probe claims a
    // complete one.
    let mut partial_engine = new_composed_engine();
    seed_ring_views(&mut partial_engine);
    let v4report = stage_v4report(false, false);
    stage_v4(&mut partial_engine, &v4report);
    seed_command_domain(&mut partial_engine);
    let mut complete_probe = contra_v4(91);
    complete_probe.inventory_complete = true;
    match partial_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("partial staged inventory must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
    assert!(
        !partial_engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "inventory refusal sets no bit",
    );
    // Negative with the eligible control above: a staged gesture
    // barrier refuses even though the contra probe claims a clean
    // lane.
    let mut barrier_engine = new_composed_engine();
    seed_ring_views(&mut barrier_engine);
    let v4report = stage_v4report(true, true);
    stage_v4(&mut barrier_engine, &v4report);
    seed_command_domain(&mut barrier_engine);
    let barrier_probe = contra_v4(92);
    match barrier_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("staged barrier must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
    assert!(
        !barrier_engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "barrier refusal sets no bit",
    );
}

#[test]
fn move_float_destination_reads_staged_screens_not_contra() {
    // Staged move float-destination screens geometry: the half-snap rect
    // derives from the staged screen work area, never the contra report's
    // default geometry; a staged topology missing the home output refuses
    // even when the contra report carries that screen.
    let custom = crate::world_report::ScreenReport {
        output: OutputId("out-a".to_owned()),
        full_rect: Some(rect(100, 0, 600, 600)),
        work_area: Some(rect(100, 0, 600, 500)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    };
    let mut v4report = report_v4_simple(2, vec![custom]);
    v4report.windows = vec![fact("win-a", "out-a", 0)];
    v4report.focused_global = "win-a".to_owned();
    v4report.foreground_known = true;
    let mut engine = new_engine();
    seed_ring_views(&mut engine);
    stage_v4(&mut engine, &v4report);
    seed_float_domain(&mut engine);
    // The contra report carries the default work area on the same
    // output: only the staged custom geometry can explain the rect.
    let work = rect(100, 0, 600, 500);
    let contra_default = rect(0, 0, 400, 560);
    let probe = contra_v4(90);
    match engine.execute_command(
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
            panic!("eligible float move must apply, refused {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("float move mints no focus effect"),
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
                "float half-snap applied",
            );
            assert_eq!(*target, Some(WindowId("win-a".to_owned())));
            let applied = rect.as_ref().expect("half-snap rect");
            assert!(
                applied.x >= work.x
                    && applied.y >= work.y
                    && applied.x + applied.w <= work.x + work.w
                    && applied.y + applied.h <= work.y + work.h,
                "rect {applied:?} stays inside the staged custom work area",
            );
            assert!(
                *applied != contra_default,
                "rect {applied:?} must not match the contra default geometry",
            );
        }
        other => panic!("expected directional result, got {other:?}"),
    }
    // Negative: staged screens lack the home output while the contra
    // report carries it, so the move refuses on staged topology.
    let mut missing_engine = new_engine();
    seed_ring_views(&mut missing_engine);
    let mut off_v4 = report_v4_simple(2, vec![screen("out-b", 800)]);
    off_v4.windows = vec![fact("win-a", "out-a", 0)];
    off_v4.focused_global = "win-a".to_owned();
    off_v4.foreground_known = true;
    stage_v4(&mut missing_engine, &off_v4);
    seed_float_domain(&mut missing_engine);
    let mut home_probe = contra_v4(91);
    home_probe.screens = vec![screen("out-a", 0)];
    match missing_engine.execute_command(
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
            panic!("missing staged screen must no-effect, refused {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("refusal-path mints no focus effect"),
    }
    match &missing_engine.command_result {
        Some(crate::world_report::CommandResult::Directional { outcome, .. }) => assert_eq!(
            *outcome,
            crate::world_directional::DirectionalOutcome::MoveFloatRefusedUnknownOutput,
            "staged topology refusal despite contra screen",
        ),
        other => panic!("expected directional refusal, got {other:?}"),
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
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
        v4report.windows = vec![fact("win-a", "out-a", 0)];
        v4report.focused_global = "win-a".to_owned();
        v4report.foreground_known = true;
        stage_v4(&mut engine, &v4report);
        seed_command_domain(&mut engine);
        let probe = contra_v4(90);
        match engine.execute_command(
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
                panic!("eligible view switch must move, refused {kind} {message}")
            }
            CommandApplied::Effect(_) => panic!("view switch mints no focus effect"),
        }
        assert_eq!(
            engine.views.get("out-a"),
            Some(&"ws-9".to_owned()),
            "target view moves (mode={mode:?})",
        );
        assert_eq!(
            engine.views.get("out-b"),
            Some(&(if fanout { "ws-9" } else { "ws-1" }).to_owned()),
            "second output follows only under Shared fanout (mode={mode:?})",
        );
    }
}

#[test]
fn global_unique_select_swap_gated_by_staged_inventory_not_contra() {
    // GlobalUnique select swaps the views visible on two outputs. One
    // retained member forces the stateful swap path where the staged
    // inventory gate lives; the swap then refuses on staged incomplete
    // inventory even though the contra report claims a complete one.
    // (The empty-topology fast path carries no inventory gate by design.)
    let stage_v4report = |inventory_complete: bool| {
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
        v4report.windows = vec![fact("win-a", "out-a", 0)];
        v4report.focused_global = "win-a".to_owned();
        v4report.foreground_known = true;
        v4report.inventory_complete = inventory_complete;
        v4report
    };
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
    let v4report = stage_v4report(true);
    stage_v4(&mut engine, &v4report);
    let probe = contra_v4(90);
    let applied = engine.execute_command(
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
            panic!("eligible swap must move, refused {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("eligible swap invents no focus"),
    }
    assert_eq!(
        engine.views.get("out-a"),
        Some(&"ws-9".to_owned()),
        "select lands on the target",
    );
    assert_eq!(
        engine.views.get("out-b"),
        Some(&"ws-1".to_owned()),
        "other output takes the displaced view",
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
    let v4report = stage_v4report(false);
    stage_v4(&mut held_engine, &v4report);
    let mut complete_probe = contra_v4(91);
    complete_probe.inventory_complete = true;
    match held_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("incomplete staged inventory must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
    assert_eq!(
        held_engine.views.get("out-a"),
        Some(&"ws-1".to_owned()),
        "refusal leaves views untouched",
    );
}

#[test]
fn migrate_mode_and_inventory_gates_read_staged_not_contra() {
    // Whole-workspace migration gates discriminate staged state against
    // the contra report: the eligible floating carry commits, Shared
    // mode refuses before any other gate, and incomplete staged
    // inventory refuses even when the contra report claims completeness.
    // (This helper reads no gesture barrier; the composed maximize lane
    // above pins that gate separately.)
    let stage_v4report = |inventory_complete: bool| {
        let mut v4report = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
        v4report.windows = vec![fact("win-a", "out-a", 0)];
        v4report.focused_global = "win-a".to_owned();
        v4report.foreground_known = true;
        v4report.inventory_complete = inventory_complete;
        v4report
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
    // Eligible: the floating source workspace carries to out-b through
    // the sessionless membership remap. The contra probe carries one
    // default screen, so the staged topology alone explains the move.
    let mut engine = new_engine();
    let v4report = stage_v4report(true);
    stage_v4(&mut engine, &v4report);
    seed_migrate_joins(&mut engine);
    let probe = contra_v4(90);
    match engine.execute_command(
        &test_issue(&probe),
        12,
        &migrate(),
        &BTreeSet::new(),
        None,
        None,
    ) {
        CommandApplied::NoEffect => {}
        CommandApplied::Rejected { kind, message } => {
            panic!("eligible migration must carry, refused {kind} {message}")
        }
        CommandApplied::Effect(_) => panic!("migration mints no focus effect"),
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
                "staged source member carried: {members:?}",
            );
        }
        other => panic!("expected migration provenance, got {other:?}"),
    }
    assert_eq!(
        engine.views.get("out-b"),
        Some(&"ws-1".to_owned()),
        "target view lands",
    );
    assert!(
        !engine
            .mapping
            .get("out-a")
            .is_some_and(|ring| ring.iter().any(|id| id == "ws-1")),
        "source ring drops the id: {:?}",
        engine.mapping.get("out-a"),
    );
    assert!(
        engine.retained.iter().any(|transfer| {
            matches!(
                &transfer.target.kind,
                crate::world_report::TransferKind::Migration
            ) && transfer.target.operation_id == "op-12"
        }),
        "migration flight retained",
    );
    // Negative with the eligible setup above: Shared mode refuses
    // with its own vocabulary.
    let mut shared_engine = new_engine();
    let v4report = stage_v4report(true);
    stage_v4(&mut shared_engine, &v4report);
    seed_migrate_joins(&mut shared_engine);
    shared_engine.settings.workspace_mode = WorkspaceMode::Shared;
    let shared_probe = contra_v4(90);
    match shared_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("shared migration must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
    // Negative with the eligible setup above: incomplete staged
    // inventory refuses while the contra report claims completeness.
    let mut partial_engine = new_engine();
    let v4report = stage_v4report(false);
    stage_v4(&mut partial_engine, &v4report);
    seed_migrate_joins(&mut partial_engine);
    let mut complete_probe = contra_v4(91);
    complete_probe.inventory_complete = true;
    match partial_engine.execute_command(
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
        CommandApplied::NoEffect => panic!("partial staged inventory must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
    assert_eq!(
        partial_engine.views.get("out-b"),
        Some(&"ws-9".to_owned()),
        "refusal leaves views untouched",
    );
}

#[test]
fn send_source_freeze_prefers_retained_home_over_current_action() {
    // The implicit-send freeze reads the source report context, never
    // stale staged state: retained homing wins over the current action
    // context, while the current action/cursor still resolves from the
    // source (not the stale staged chain). Source-freeze survival through
    // staging end to end is covered by the accepted-run freeze tests.
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
    let mut stale_v4 = report_v4_simple(2, vec![screen("out-a", 0), screen("out-b", 800)]);
    stale_v4.windows = vec![fact("win-a", "out-a", 0)];
    stale_v4.action_foreground = None;
    stale_v4.action_cached = Some(OutputId("out-a".to_owned()));
    stale_v4.action_outputs = vec![OutputId("out-a".to_owned())];
    stage_v4(&mut engine, &stale_v4);
    // Current source context: foreground and cursor both on out-b
    // with a registry ordering out-b first.
    let mut current = report_v4_simple(3, vec![screen("out-a", 0), screen("out-b", 800)]);
    current.windows = vec![fact("win-a", "out-a", 0)];
    current.action_foreground = Some(OutputId("out-b".to_owned()));
    current.cursor = Some((850, 10));
    current.action_outputs = vec![OutputId("out-b".to_owned()), OutputId("out-a".to_owned())];
    let source_ctx = RawRouting::from_v4(&current);
    let staged_ctx = RawRouting::from_staged(&engine.staged);
    assert_eq!(
        engine.resolve_home_then_action(&source_ctx, &WindowId("win-a".to_owned())),
        Some(OutputId("out-a".to_owned())),
        "retained home wins over current action",
    );
    assert_eq!(
        engine.resolve_action_output(&source_ctx),
        Some(OutputId("out-b".to_owned())),
        "current action resolves from the source context",
    );
    assert_eq!(
        engine.resolve_action_output(&staged_ctx),
        Some(OutputId("out-a".to_owned())),
        "stale staged chain disagrees, proving the freeze must use the source",
    );
    // Unfrozen helper plumbing refuses without inventing a source;
    // the frozen source resolves verbatim.
    let probe = contra_v4(90);
    match engine.execute_command(
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
        CommandApplied::NoEffect => panic!("unfrozen implicit source must refuse"),
        CommandApplied::Effect(_) => panic!("refusal mints no effect"),
    }
}

#[test]
fn fresh_focus_current_retained_native_and_primary_order() {
    // Retained join: win-a homed on out-a. Staged facts carry win-a
    // plus a brand-new win-new on out-b; live screens on three
    // outputs with the designated primary (out-c) kept clear of the
    // focus legs, so each basis resolves to its own output.
    let setup = |focused: &str| {
        let mut v4report = report_v4_simple(
            3,
            vec![
                screen("out-a", 0),
                screen("out-b", 800),
                screen("out-c", 1600),
            ],
        );
        v4report.windows = vec![fact("win-a", "out-a", 0), fact("win-new", "out-b", 800)];
        v4report.focused_global = focused.to_owned();
        v4report.foreground_known = !focused.is_empty();
        v4report.primary_output = Some(OutputId("out-c".to_owned()));
        v4report
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
    let v4report = setup("win-a");
    stage_v4(&mut engine, &v4report);
    let (current, trace) = engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
    assert_eq!(current, Some(OutputId("out-a".to_owned())), "current wins");
    assert_eq!(
        trace.basis,
        crate::fresh_focus::FreshFocusBasis::Current,
        "current basis",
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
    let v4report = setup("");
    stage_v4(&mut engine, &v4report);
    let (retained, trace) = engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
    assert_eq!(
        retained,
        Some(OutputId("out-b".to_owned())),
        "retained wins"
    );
    assert_eq!(
        trace.basis,
        crate::fresh_focus::FreshFocusBasis::Retained,
        "retained basis",
    );
    // No latch: the brand-new native fact wins over primary, on a
    // different output than the designated primary below.
    let mut engine = new_engine();
    let v4report = setup("win-new");
    stage_v4(&mut engine, &v4report);
    let (native, trace) = engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
    assert_eq!(native, Some(OutputId("out-b".to_owned())), "native wins");
    assert_eq!(
        trace.basis,
        crate::fresh_focus::FreshFocusBasis::Native,
        "native basis",
    );
    let ordered = engine.primary_first_outputs(&RawRouting::from_staged(&engine.staged));
    assert_eq!(
        ordered.first(),
        Some(&OutputId("out-c".to_owned())),
        "primary chain disagrees with the native pick",
    );
    // Nothing observed: no fresh focus; the primary-first chain still
    // names the designated output first.
    let mut engine = new_engine();
    let v4report = setup("");
    stage_v4(&mut engine, &v4report);
    let (none, trace) = engine.fresh_focused_output(&RawRouting::from_staged(&engine.staged));
    assert_eq!(none, None, "no invention");
    assert_eq!(
        trace.basis,
        crate::fresh_focus::FreshFocusBasis::Primary,
        "primary basis",
    );
    let ordered = engine.primary_first_outputs(&RawRouting::from_staged(&engine.staged));
    assert_eq!(
        ordered.first(),
        Some(&OutputId("out-c".to_owned())),
        "designated primary first",
    );
}

#[test]
fn switch_view_command_moves_view_parity() {
    // View selection through the migrated command helper on the default
    // local mode: the staged ring and screens authorize the move, with
    // no invented focus.
    let mut engine = new_engine();
    seed_ring_views(&mut engine);
    let mut v4report = report_v4_simple(2, vec![screen("out-a", 0)]);
    v4report.windows = vec![fact("win-a", "out-a", 0)];
    v4report.focused_global = "win-a".to_owned();
    v4report.foreground_known = true;
    stage_v4(&mut engine, &v4report);
    seed_command_domain(&mut engine);
    let probe = contra_v4(90);
    let command = WorldCommand::SwitchView {
        output: OutputId("out-a".to_owned()),
        workspace: "ws-9".to_owned(),
    };
    let applied = engine.execute_command(
        &test_issue(&probe),
        12,
        &command,
        &BTreeSet::new(),
        None,
        None,
    );
    assert!(
        matches!(applied, CommandApplied::NoEffect),
        "empty target view moves with no invented focus",
    );
    assert_eq!(
        engine.views.get("out-a"),
        Some(&"ws-9".to_owned()),
        "view moved on the staged intake",
    );
}

#[test]
fn handle_report_echoes_envelope_request_and_command_sequence() {
    // Direct v4 entry: the reply echoes the envelope request id and the
    // one-shot effect carries the dispatching command sequence; an
    // observation-only report stays accepted with no effects and no
    // unconditional failure.
    let mut engine = new_engine();
    let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
    bootstrap.windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    bootstrap.focused_global = "win-a".to_owned();
    bootstrap.foreground_known = true;
    match engine.handle_report(&bootstrap) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("bootstrap must accept, got {other:?}"),
    }
    let mut command_report = report_v4_simple(2, vec![screen("out-a", 0)]);
    command_report.windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    command_report.focused_global = "win-a".to_owned();
    command_report.foreground_known = true;
    command_report.command_seq = Some(7);
    command_report.command = Some(focus_command("win-b"));
    let reply = match engine.handle_report(&command_report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("command report must accept, got {other:?}"),
    };
    assert_eq!(reply.request_id, "req-2");
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target, WindowId("win-b".to_owned()));
    assert_eq!(reply.effects[0].command_seq, Some(7));
    assert_eq!(reply.effects[0].cause, EffectCause::ExplicitCommand);
    let mut observation_only = report_v4_simple(3, vec![screen("out-a", 0)]);
    observation_only.windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 200)];
    observation_only.focused_global = "win-a".to_owned();
    observation_only.foreground_known = true;
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

#[test]
fn hidden_focus_target_refuses_complete_and_incomplete() {
    // Staged minimized/host-hidden focus targets refuse with the
    // existing focus vocabulary even though retained membership still
    // authorizes them; the visible control effects, proving no blanket
    // focus ban. Raw hidden flags are set on the source v4 reports
    // BEFORE intake, and the contra probe carries no hidden state, so
    // only staged inputs can explain the outcomes. Both inventory legs
    // stage the same hidden truth: the guard reads per-window staged
    // facts, never a report-wide inventory verdict. Minimized
    // restoration rides its own lane; nothing here restores.
    for (hidden_kind, inventory_complete) in [
        ("minimized", true),
        ("minimized", false),
        ("host-hidden", true),
        ("host-hidden", false),
    ] {
        let mut engine = new_engine();
        let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
        bootstrap.windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
        bootstrap.focused_global = "win-a".to_owned();
        bootstrap.foreground_known = true;
        accepted_v4(&mut engine, &bootstrap);
        let mut hidden_report = report_v4_simple(2, vec![screen("out-a", 0)]);
        let mut hidden_win = fact("win-b", "out-a", 200);
        if hidden_kind == "minimized" {
            hidden_win.minimized = Some(true);
        } else {
            hidden_win.host_hidden = Some(true);
        }
        hidden_report.windows = vec![fact("win-a", "out-a", 0), hidden_win];
        hidden_report.focused_global = "win-a".to_owned();
        hidden_report.foreground_known = true;
        hidden_report.inventory_complete = inventory_complete;
        stage_v4(&mut engine, &hidden_report);
        seed_ring_views(&mut engine);
        // Real command-planning joins over the staged facts
        // (classification only, from staged state with no report
        // input), mirroring accepted-run assembly: hidden rows land in
        // the minimized lane, never the carrier index.
        engine.build_staged_domains();
        assert!(
            engine.staged_hidden(&WindowId("win-b".to_owned())),
            "{hidden_kind} target stages hidden (complete={inventory_complete})",
        );
        assert!(
            !engine.staged_hidden(&WindowId("win-a".to_owned())),
            "visible control stages shown (complete={inventory_complete})",
        );
        let mut owned: Vec<EngineWindow> = Vec::new();
        for domain in &engine.staged.domains {
            owned.extend(domain.windows.iter().cloned());
        }
        let by_id: BTreeMap<&WindowId, &EngineWindow> =
            owned.iter().map(|row| (&row.window, row)).collect();
        // Load-bearing shape: the hidden target leaves the staged
        // carrier index while retained membership still authorizes it,
        // so only the staged-hidden guard can explain the refusal.
        assert!(
            !by_id.contains_key(&WindowId("win-b".to_owned())),
            "{hidden_kind} target leaves the carrier index (complete={inventory_complete})",
        );
        assert!(
            engine.engine.domain_keys().iter().any(|key| {
                engine.engine.session(key).is_some_and(|session| {
                    session
                        .snapshot()
                        .windows
                        .iter()
                        .any(|link| link.window == WindowId("win-b".to_owned()))
                        || session.is_exception(&WindowId("win-b".to_owned()))
                })
            }),
            "{hidden_kind} target keeps retained membership (complete={inventory_complete})",
        );
        let probe = contra_v4(90);
        match engine.execute_command(
            &test_issue(&probe),
            12,
            &focus_command("win-b"),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_FOCUS),
            CommandApplied::NoEffect => {
                panic!("{hidden_kind} focus must refuse (complete={inventory_complete})")
            }
            CommandApplied::Effect(_) => {
                panic!("{hidden_kind} focus must never effect (complete={inventory_complete})")
            }
        }
        // Visible control: no blanket focus ban while hidden is staged.
        match engine.execute_command(
            &test_issue(&probe),
            13,
            &focus_command("win-a"),
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::Effect(effect) => {
                assert_eq!(effect.target, WindowId("win-a".to_owned()));
                assert_eq!(effect.command_seq, Some(13));
            }
            CommandApplied::Rejected { kind, message } => panic!(
                "visible focus must effect with {hidden_kind} staged, refused {kind} {message}"
            ),
            CommandApplied::NoEffect => panic!(
                "visible focus must effect with {hidden_kind} staged (complete={inventory_complete})"
            ),
        }
        // Observations preserved: the hidden row stays staged verbatim
        // with no controlled geometry and no hidden focus recorded.
        let staged = engine
            .staged
            .facts
            .get(&WindowId("win-b".to_owned()))
            .expect("hidden observation retained");
        if hidden_kind == "minimized" {
            assert_eq!(staged.minimized, Some(true));
        } else {
            assert_eq!(staged.host_hidden, Some(true));
        }
        assert!(
            engine.implicit_focus != Some(WindowId("win-b".to_owned())),
            "refused hidden focus records nothing",
        );
    }
}

#[test]
fn hidden_directional_source_refuses_stale_complete_and_incomplete() {
    // A staged minimized/host-hidden directional-focus source refuses
    // with the existing stale vocabulary, like an unknown window (no
    // new kind, no wire change). Same four legs as the direct guard:
    // minimized/host-hidden across complete/incomplete inventory, with
    // a visible-source control proving the guard never misfires.
    for (hidden_kind, inventory_complete) in [
        ("minimized", true),
        ("minimized", false),
        ("host-hidden", true),
        ("host-hidden", false),
    ] {
        let mut engine = new_engine();
        let mut bootstrap = report_v4_simple(1, vec![screen("out-a", 0)]);
        bootstrap.windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
        bootstrap.focused_global = "win-a".to_owned();
        bootstrap.foreground_known = true;
        accepted_v4(&mut engine, &bootstrap);
        let mut hidden_report = report_v4_simple(2, vec![screen("out-a", 0)]);
        let mut hidden_win = fact("win-b", "out-a", 200);
        if hidden_kind == "minimized" {
            hidden_win.minimized = Some(true);
        } else {
            hidden_win.host_hidden = Some(true);
        }
        hidden_report.windows = vec![fact("win-a", "out-a", 0), hidden_win];
        hidden_report.focused_global = "win-a".to_owned();
        hidden_report.foreground_known = true;
        hidden_report.inventory_complete = inventory_complete;
        stage_v4(&mut engine, &hidden_report);
        seed_ring_views(&mut engine);
        engine.build_staged_domains();
        let probe = contra_v4(90);
        match engine.execute_command(
            &test_issue(&probe),
            12,
            &WorldCommand::FocusDirectional {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            },
            &BTreeSet::new(),
            None,
            None,
        ) {
            CommandApplied::NoEffect => {}
            CommandApplied::Rejected { kind, message } => panic!(
                "{hidden_kind} directional source must stale-no-effect, refused {kind} {message}"
            ),
            CommandApplied::Effect(_) => panic!(
                "{hidden_kind} directional source must never effect (complete={inventory_complete})"
            ),
        }
        match &engine.command_result {
            Some(crate::world_report::CommandResult::Directional {
                outcome,
                target,
                cause,
                ..
            }) => {
                assert_eq!(
                    *outcome,
                    crate::world_directional::DirectionalOutcome::FocusFloatRefusedStale,
                    "{hidden_kind} stale outcome (complete={inventory_complete})",
                );
                assert_eq!(*target, Some(WindowId("win-b".to_owned())));
                assert_eq!(cause.as_deref(), Some("unknown-window"));
            }
            other => panic!(
                "{hidden_kind} directional source must keep the stale vocabulary, got {other:?}"
            ),
        }
        // Visible-source control: the guard never misfires on a shown
        // subject, so its result never takes the hidden stale path.
        let applied = engine.execute_command(
            &test_issue(&probe),
            13,
            &WorldCommand::FocusDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
            },
            &BTreeSet::new(),
            None,
            None,
        );
        let stale_hidden_path = matches!(applied, CommandApplied::NoEffect)
            && matches!(
                &engine.command_result,
                Some(crate::world_report::CommandResult::Directional {
                    outcome,
                    cause,
                    ..
                }) if *outcome
                    == crate::world_directional::DirectionalOutcome::FocusFloatRefusedStale
                    && cause.as_deref() == Some("unknown-window")
            );
        assert!(
            !stale_hidden_path,
            "visible directional source must not take the hidden stale path ({hidden_kind}, complete={inventory_complete})",
        );
    }
}
