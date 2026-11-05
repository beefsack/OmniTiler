//! v4 pre-observation send/migration freezes (7b-3): the shared freeze
//! helper resolves implicit send sources and direction-only migration
//! sources from prior retained homes/views against borrowed raw routing,
//! with identical outcomes on the legacy and genuine v4 routing lanes.
//! Nothing here ingests intent or assembles a v4 accepted run: the
//! command-level checks stage reports (legacy intake or the genuine v4
//! intake plus the shared pre-convergence) only to prove the frozen
//! values still refuse through the unchanged staged gates in
//! `execute_command`. A same-report-created view never retroactively
//! authorizes a frozen refusal, and a frozen source whose output left
//! live topology refuses at the staged topology gate instead of
//! retargeting.

use std::collections::BTreeSet;

use crate::directional::{OutputId, WindowId};
use crate::ids::{CorrelationId, OwnerId};
use crate::session::RefusalKind;
use crate::world_commands::CommandApplied;
use crate::world_directional::tests::{accepted, fact, report_full, screen};
use crate::world_engine::{
    EventIssue, KIND_MIGRATION, MSG_MIGRATION_SOURCE, MSG_MIGRATION_TOPOLOGY_UNREADABLE,
    RawRouting, WorldEngine,
};
use crate::world_report::{
    PresentationIngressV4, PresentationRowV4, ScreenReport, SendSource, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorldCommand, WorldReport, WorldReportV4,
};

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn seed_homed(engine: &mut WorldEngine) {
    accepted(
        engine,
        &report_full(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
}

fn legacy_full(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    foreground: Option<&str>,
    cached: Option<&str>,
    registry: Vec<&str>,
    cursor: Option<(i32, i32)>,
) -> WorldReport {
    WorldReport {
        cursor,
        action_foreground: foreground.map(|out| OutputId(out.to_owned())),
        action_cached: cached.map(|out| OutputId(out.to_owned())),
        action_outputs: registry
            .into_iter()
            .map(|out| OutputId(out.to_owned()))
            .collect(),
        ..report_full(seq, seq, screens, windows, vec![], "", None, None)
    }
}

fn v4_full(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    foreground: Option<&str>,
    cached: Option<&str>,
    registry: Vec<&str>,
    cursor: Option<(i32, i32)>,
) -> WorldReportV4 {
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
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: foreground.map(|out| OutputId(out.to_owned())),
        action_cached: cached.map(|out| OutputId(out.to_owned())),
        action_outputs: registry
            .into_iter()
            .map(|out| OutputId(out.to_owned()))
            .collect(),
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

fn homed_windows() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0)]
}

fn implicit_numbered(window: &str) -> WorldCommand {
    WorldCommand::SendNumbered {
        window: WindowId(window.to_owned()),
        source: SendSource::HomeThenAction,
        index: 0,
        follow: false,
    }
}

fn implicit_relative(window: &str) -> WorldCommand {
    WorldCommand::SendRelative {
        window: WindowId(window.to_owned()),
        source: SendSource::HomeThenAction,
        delta: 1,
        follow: false,
    }
}

/// One frozen pre-observation command source pair: the implicit send
/// source plus the direction-only migration source and its retained view.
type FrozenSources = (Option<OutputId>, Option<(OutputId, String)>);

fn freezes(
    engine: &WorldEngine,
    command: Option<&WorldCommand>,
    legacy: &WorldReport,
    v4: &WorldReportV4,
) -> (FrozenSources, FrozenSources) {
    let legacy_ctx = RawRouting::from_source(legacy);
    let v4_ctx = RawRouting::from_v4(v4);
    (
        engine.freeze_command_sources(command, &legacy_ctx),
        engine.freeze_command_sources(command, &v4_ctx),
    )
}

fn issue(seq: u64) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&format!("req-{seq}")).expect("correlation"),
    }
}

#[test]
fn freeze_parity_retained_home_beats_disagreeing_action() {
    // Homed mover wins over a disagreeing pointer/cache action context on
    // both intakes, for both implicit send variants.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    for command in [implicit_numbered("win-a"), implicit_relative("win-a")] {
        let legacy = legacy_full(
            2,
            two_screens(),
            homed_windows(),
            None,
            Some("out-b"),
            vec!["out-a", "out-b"],
            Some((810, 10)),
        );
        let v4 = v4_full(
            2,
            two_screens(),
            homed_windows(),
            None,
            Some("out-b"),
            vec!["out-a", "out-b"],
            Some((810, 10)),
        );
        let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
        let expected = Some(OutputId("out-a".to_owned()));
        assert_eq!(legacy_frozen.0, expected, "legacy must pin retained home");
        assert_eq!(v4_frozen.0, expected, "v4 must pin retained home");
        assert_eq!(legacy_frozen, v4_frozen, "dual-intake parity");
        assert_eq!(
            legacy_frozen.1, None,
            "send-only command freezes no migration"
        );
    }
}

#[test]
fn freeze_action_precedence_foreground_pointer_cache_registry() {
    // Unhomed lifetimes resolve the action chain
    // foreground-over-pointer-over-cache-over-registry-first identically on
    // both intakes. Cursor (10, 10) lands on out-a and (900, 10) on out-b.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let command = implicit_numbered("win-new");
    for (foreground, cached, cursor, expected) in [
        (Some("out-b"), Some("out-a"), Some((10, 10)), "out-b"),
        (None, Some("out-a"), Some((900, 10)), "out-b"),
        (None, Some("out-b"), None, "out-b"),
        (None, None, None, "out-a"),
    ] {
        let legacy = legacy_full(
            2,
            two_screens(),
            homed_windows(),
            foreground,
            cached,
            vec!["out-a", "out-b"],
            cursor,
        );
        let v4 = v4_full(
            2,
            two_screens(),
            homed_windows(),
            foreground,
            cached,
            vec!["out-a", "out-b"],
            cursor,
        );
        let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
        let expected = Some(OutputId(expected.to_owned()));
        assert_eq!(legacy_frozen.0, expected, "legacy precedence");
        assert_eq!(v4_frozen.0, expected, "v4 precedence");
        assert_eq!(legacy_frozen, v4_frozen, "dual-intake parity");
    }
}

#[test]
fn freeze_explicit_send_bypasses_both_lanes() {
    // Explicit outputs never freeze: both lanes stay empty for both send
    // variants, so the staged gate below uses the named output verbatim.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let explicit = SendSource::Explicit(OutputId("out-b".to_owned()));
    for command in [
        WorldCommand::SendNumbered {
            window: WindowId("win-a".to_owned()),
            source: explicit.clone(),
            index: 0,
            follow: false,
        },
        WorldCommand::SendRelative {
            window: WindowId("win-a".to_owned()),
            source: explicit.clone(),
            delta: 1,
            follow: false,
        },
    ] {
        let legacy = legacy_full(
            2,
            two_screens(),
            homed_windows(),
            Some("out-a"),
            Some("out-b"),
            vec!["out-a", "out-b"],
            None,
        );
        let v4 = v4_full(
            2,
            two_screens(),
            homed_windows(),
            Some("out-a"),
            Some("out-b"),
            vec!["out-a", "out-b"],
            None,
        );
        let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
        assert_eq!(legacy_frozen, (None, None), "explicit send freezes nothing");
        assert_eq!(v4_frozen, (None, None), "explicit send freezes nothing");
    }
}

#[test]
fn freeze_unhomed_action_chain_and_empty_registry() {
    // Unhomed lifetimes fall back through the ordered action registry; an
    // empty registry with no action legs freezes nothing on both intakes.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let command = implicit_numbered("win-new");
    let legacy = legacy_full(
        2,
        two_screens(),
        homed_windows(),
        None,
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let v4 = v4_full(
        2,
        two_screens(),
        homed_windows(),
        None,
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
    let expected = Some(OutputId("out-a".to_owned()));
    assert_eq!(legacy_frozen.0, expected, "registry-first chain");
    assert_eq!(v4_frozen.0, expected, "registry-first chain");
    assert_eq!(legacy_frozen, v4_frozen, "dual-intake parity");
    let legacy_empty = legacy_full(3, two_screens(), homed_windows(), None, None, vec![], None);
    let v4_empty = v4_full(3, two_screens(), homed_windows(), None, None, vec![], None);
    let (legacy_none, v4_none) = freezes(&engine, Some(&command), &legacy_empty, &v4_empty);
    assert_eq!(legacy_none, (None, None), "no legs and no registry");
    assert_eq!(v4_none, (None, None), "no legs and no registry");
}

#[test]
fn freeze_stale_action_pins_without_fallback() {
    // A registry-retained stale choice pins verbatim on both intakes: the
    // send freeze keeps the stale output instead of retargeting live, and
    // the migration freeze stays empty because the stale output owns no
    // retained view.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let command = implicit_numbered("win-new");
    let legacy = legacy_full(
        2,
        two_screens(),
        homed_windows(),
        None,
        Some("out-zzz"),
        vec!["out-zzz", "out-a"],
        None,
    );
    let v4 = v4_full(
        2,
        two_screens(),
        homed_windows(),
        None,
        Some("out-zzz"),
        vec!["out-zzz", "out-a"],
        None,
    );
    let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
    let expected_send = Some(OutputId("out-zzz".to_owned()));
    assert_eq!(legacy_frozen.0, expected_send, "stale choice pins");
    assert_eq!(v4_frozen.0, expected_send, "stale choice pins");
    assert_eq!(legacy_frozen, v4_frozen, "dual-intake parity");
    let migrate = WorldCommand::MigrateDirectional {
        direction: "left".to_owned(),
    };
    let (legacy_mig, v4_mig) = freezes(&engine, Some(&migrate), &legacy, &v4);
    assert_eq!(
        legacy_mig,
        (None, None),
        "stale output owns no retained view"
    );
    assert_eq!(v4_mig, (None, None), "stale output owns no retained view");
}

#[test]
fn freeze_migration_binds_action_output_not_home() {
    // Direction-only migration sources the action output's retained view,
    // never the focused window's home: the two disagree here on purpose.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let view_b = engine
        .logical_view("out-b")
        .expect("retained view for out-b");
    let command = WorldCommand::MigrateDirectional {
        direction: "left".to_owned(),
    };
    // win-a is homed on out-a, but the action context points at out-b.
    let legacy = legacy_full(
        2,
        two_screens(),
        homed_windows(),
        Some("out-b"),
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let v4 = v4_full(
        2,
        two_screens(),
        homed_windows(),
        Some("out-b"),
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
    let expected = (None, Some((OutputId("out-b".to_owned()), view_b)));
    assert_eq!(legacy_frozen, expected, "migration binds action view");
    assert_eq!(v4_frozen, expected, "migration binds action view");
}

#[test]
fn freeze_migration_unbootstrapped_and_new_view_refuse() {
    // No retained view means no migration source on both intakes: a fresh
    // engine refuses even though the routing names live screens.
    let fresh = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let command = WorldCommand::MigrateDirectional {
        direction: "left".to_owned(),
    };
    let legacy = legacy_full(
        1,
        two_screens(),
        Vec::new(),
        Some("out-a"),
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let v4 = v4_full(
        1,
        two_screens(),
        Vec::new(),
        Some("out-a"),
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let (legacy_frozen, v4_frozen) = freezes(&fresh, Some(&command), &legacy, &v4);
    assert_eq!(
        legacy_frozen,
        (None, None),
        "unbootstrapped migration refuses"
    );
    assert_eq!(v4_frozen, (None, None), "unbootstrapped migration refuses");
}

#[test]
fn frozen_migration_new_view_cannot_authorize_after_v4_converge() {
    // The hotplug screen is genuinely present in the v4 routing rows, so
    // the pre-observation freeze sees the action resolve yet still binds
    // nothing for lack of a retained view. Genuine v4 staging plus the
    // shared pre-convergence then creates that view, and the frozen None
    // still refuses through `execute_command`: the same-report view
    // cannot retroactively authorize the intent.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    assert!(
        engine.logical_view("out-zzz").is_none(),
        "precondition: no retained view for the new screen"
    );
    let command = WorldCommand::MigrateDirectional {
        direction: "left".to_owned(),
    };
    let hotplug_screens = vec![
        screen("out-a", 0),
        screen("out-b", 800),
        screen("out-zzz", 1600),
    ];
    let legacy = legacy_full(
        2,
        hotplug_screens.clone(),
        homed_windows(),
        Some("out-zzz"),
        None,
        vec!["out-a", "out-b", "out-zzz"],
        None,
    );
    let v4 = v4_full(
        2,
        hotplug_screens,
        homed_windows(),
        Some("out-zzz"),
        None,
        vec!["out-a", "out-b", "out-zzz"],
        None,
    );
    let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
    assert_eq!(
        legacy_frozen,
        (None, None),
        "same-report view refuses on legacy lanes"
    );
    assert_eq!(
        v4_frozen,
        (None, None),
        "same-report view refuses on v4 lanes"
    );
    // Genuine v4 staging plus shared pre-convergence creates the view the
    // freeze predates.
    engine.stage_facts_v4(&v4);
    engine.converge_logical_pre();
    engine.build_staged_domains();
    assert!(
        engine.logical_view("out-zzz").is_some(),
        "convergence created the new view the freeze predates"
    );
    match engine.execute_command(
        &issue(2),
        12,
        &command,
        &BTreeSet::new(),
        v4_frozen.0,
        v4_frozen.1,
    ) {
        CommandApplied::Rejected { kind, message } => {
            assert_eq!(kind, KIND_MIGRATION, "frozen refusal kind");
            assert_eq!(message, MSG_MIGRATION_SOURCE, "frozen refusal message");
        }
        CommandApplied::Effect(_) | CommandApplied::NoEffect => {
            panic!("frozen None must refuse despite the converged view")
        }
    }
}

#[test]
fn frozen_migration_unplugged_source_refuses_at_staged_topology_gate() {
    // The freeze binds the prior retained (out-b, view) tuple even though
    // the new routing already dropped out-b: freezing predates staging.
    // Genuine v4 staging plus shared pre-convergence then runs, and the
    // frozen tuple refuses at the staged topology gate instead of
    // retargeting a live output.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let view_b = engine
        .logical_view("out-b")
        .expect("retained view for out-b");
    let command = WorldCommand::MigrateDirectional {
        direction: "left".to_owned(),
    };
    let legacy = legacy_full(
        2,
        vec![screen("out-a", 0)],
        homed_windows(),
        Some("out-b"),
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let v4 = v4_full(
        2,
        vec![screen("out-a", 0)],
        homed_windows(),
        Some("out-b"),
        None,
        vec!["out-a", "out-b"],
        None,
    );
    let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
    let expected = (None, Some((OutputId("out-b".to_owned()), view_b)));
    assert_eq!(legacy_frozen, expected, "freeze predates staging");
    assert_eq!(v4_frozen, expected, "freeze predates staging");
    // Real v4 stage plus shared pre-convergence, then the frozen tuple
    // through the unchanged staged gate. `retain_live_topology` mirrors
    // the accepted path, which retains the report's own geometry before
    // command planning so the gate below reads staged topology.
    engine.stage_facts_v4(&v4);
    engine.converge_logical_pre();
    engine.build_staged_domains();
    engine.retain_live_topology();
    match engine.execute_command(
        &issue(2),
        12,
        &command,
        &BTreeSet::new(),
        v4_frozen.0,
        v4_frozen.1,
    ) {
        CommandApplied::Rejected { kind, message } => {
            assert_eq!(kind, KIND_MIGRATION, "staged topology refusal kind");
            assert_eq!(
                message, MSG_MIGRATION_TOPOLOGY_UNREADABLE,
                "unplugged source refuses instead of retargeting"
            );
        }
        CommandApplied::Effect(_) | CommandApplied::NoEffect => {
            panic!("unplugged frozen source must refuse, never retarget")
        }
    }
}

#[test]
fn freeze_neither_ingests_nor_stages() {
    // The helper is read-only: pending intent and staged state are
    // identical before and after freezing on both intakes.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    seed_homed(&mut engine);
    let pending_before = engine.pending_intent.len();
    let staged_seq_before = engine.staged.observation.observation_seq;
    let staged_facts_before = engine.staged.facts.len();
    let command = implicit_numbered("win-a");
    let legacy = legacy_full(
        2,
        two_screens(),
        homed_windows(),
        None,
        Some("out-b"),
        vec!["out-a", "out-b"],
        Some((810, 10)),
    );
    let v4 = v4_full(
        2,
        two_screens(),
        homed_windows(),
        None,
        Some("out-b"),
        vec!["out-a", "out-b"],
        Some((810, 10)),
    );
    let _ = freezes(&engine, Some(&command), &legacy, &v4);
    let migrate = WorldCommand::MigrateDirectional {
        direction: "left".to_owned(),
    };
    let _ = freezes(&engine, Some(&migrate), &legacy, &v4);
    assert_eq!(
        engine.pending_intent.len(),
        pending_before,
        "no intent ingested"
    );
    assert_eq!(
        engine.staged.observation.observation_seq, staged_seq_before,
        "no observation staged"
    );
    assert_eq!(
        engine.staged.facts.len(),
        staged_facts_before,
        "no facts staged"
    );
}

#[test]
fn frozen_send_refuses_staged_rehome_through_execute_command() {
    // The pre-observation freeze (retained out-a home) refuses when the
    // staged mover has relocated to out-b, on both intakes: the frozen
    // value applies verbatim through the unchanged staged gate instead of
    // following the move.
    for v4_intake in [false, true] {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        seed_homed(&mut engine);
        let command = implicit_numbered("win-a");
        let legacy = legacy_full(
            2,
            two_screens(),
            homed_windows(),
            None,
            None,
            vec!["out-a", "out-b"],
            None,
        );
        let v4 = v4_full(
            2,
            two_screens(),
            homed_windows(),
            None,
            None,
            vec!["out-a", "out-b"],
            None,
        );
        let (legacy_frozen, v4_frozen) = freezes(&engine, Some(&command), &legacy, &v4);
        let (frozen_send, intake) = if v4_intake {
            (v4_frozen.0, "v4")
        } else {
            (legacy_frozen.0, "legacy")
        };
        assert_eq!(
            frozen_send,
            Some(OutputId("out-a".to_owned())),
            "precondition ({intake})"
        );
        // The observation moves win-a to out-b; staging re-homes the mover
        // while the freeze still pins the retired out-a home.
        let moved_windows = vec![fact("win-a", "out-b", 800)];
        if v4_intake {
            engine.stage_facts_v4(&v4_full(
                2,
                two_screens(),
                moved_windows,
                None,
                None,
                vec!["out-a", "out-b"],
                None,
            ));
        } else {
            engine.stage_facts(&legacy_full(
                2,
                two_screens(),
                moved_windows,
                None,
                None,
                vec!["out-a", "out-b"],
                None,
            ));
        }
        engine.build_staged_domains();
        match engine.execute_command(&issue(2), 12, &command, &BTreeSet::new(), frozen_send, None) {
            CommandApplied::Rejected { kind, .. } => assert_eq!(
                kind,
                RefusalKind::CrossDomainMismatch.as_str(),
                "frozen source must refuse the staged rehome ({intake})"
            ),
            CommandApplied::Effect(_) | CommandApplied::NoEffect => {
                panic!("frozen source must refuse the staged rehome ({intake})")
            }
        }
    }
}
