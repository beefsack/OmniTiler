//! Composed sticky command execution: `SetSticky`/`ToggleSticky` transitions,
//! staged saves, current-workspace landing, and `ToggleFloat` clearing
//! sticky. Both-host-absent consumes with a typed `StateUnsupported`
//! rejection. Publish drain and visibility decoration are covered alongside
//! the transitions: saves drain through `drain_intent_saves` and the
//! composed show target decorates via `decorate_sticky_visibility`. Native
//! sticky production lives in `world_sticky_native_tests`.

use crate::boundary::CoreCommand;
use crate::connection::HostCapabilities;
use crate::directional::{OutputId, WindowId};
use crate::intent_data::{IntentData, decode_intent_data, encode_intent_data};
use crate::world_directional::tests::{accepted, fact, float_fact, report_full, screen};
use crate::world_engine::{LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    CommandRejectKind, CommandResult, IntentRecord, ScopeAuthority, StateProperty,
    VisibilityTarget, WindowDesire, WindowFact, WorldCommand, WorldReport, encode_reply_canonical,
};

fn composed_caps() -> HostCapabilities {
    HostCapabilities {
        native_sticky: false,
        window_suppression: true,
        maximize: true,
        ..Default::default()
    }
}

fn unsupported_caps() -> HostCapabilities {
    HostCapabilities {
        native_sticky: false,
        window_suppression: false,
        ..Default::default()
    }
}

fn composed_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", composed_caps())
        .expect("generation")
}

fn cfact(window: &str, output: &str, x: i32) -> WindowFact {
    let mut row = fact(window, output, x);
    row.sticky = None;
    row
}

fn cfloat(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
    let mut row = float_fact(window, output, x, y);
    row.sticky = None;
    row
}

fn bootstrap(windows: Vec<WindowFact>) -> WorldReport {
    report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        None,
        None,
    )
}

fn cmd(seq: u64, command_seq: u64, windows: Vec<WindowFact>, command: WorldCommand) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        Some(command_seq),
        Some(command),
    )
}

fn set_sticky(window: &str, value: bool) -> WorldCommand {
    WorldCommand::SetSticky {
        window: WindowId(window.to_owned()),
        value,
    }
}

fn toggle_sticky(window: &str) -> WorldCommand {
    WorldCommand::ToggleSticky {
        window: WindowId(window.to_owned()),
    }
}

fn toggle_float(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    })
}

fn is_floated(engine: &WorldEngine, win: &str) -> bool {
    let id = WindowId(win.to_owned());
    engine.engine.domain_keys().iter().any(|key| {
        engine
            .engine
            .session(key)
            .is_some_and(|s| s.is_exception(&id))
    })
}

fn is_tiled(engine: &WorldEngine, win: &str) -> bool {
    let id = WindowId(win.to_owned());
    engine.engine.domain_keys().iter().any(|key| {
        engine
            .engine
            .session(key)
            .is_some_and(|s| s.snapshot().windows.iter().any(|l| l.window == id))
    })
}

fn staged_save(engine: &WorldEngine, win: &str) -> IntentData {
    // Publish drains the staged queue exactly once: the save is read back
    // from the actual published reply, and the queue must be empty.
    assert!(
        engine.sticky_command_save.is_none(),
        "publish drains the staged save"
    );
    let reply = engine.last_reply.as_ref().expect("published reply");
    assert_eq!(reply.intent_writes.len(), 1, "exactly one save");
    let save = &reply.intent_writes[0];
    assert_eq!(save.win, WindowId(win.to_owned()));
    decode_intent_data(save.data.as_deref().expect("save bytes")).expect("save decodes")
}

fn assert_no_state_targets(engine: &WorldEngine) {
    assert!(
        !engine
            .retained_states
            .keys()
            .any(|(_, property)| *property == StateProperty::Sticky),
        "no native sticky bindings on the composed route"
    );
}

#[test]
fn set_sticky_on_from_tiled_floats_with_false_prior_save() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    assert!(is_tiled(&engine, "win-a"));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(!is_tiled(&engine, "win-a"));
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(true), Some(false))
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(true), Some(false)))
    );
    // Raw facts never rewrite: the staged fact stays sticky-null.
    assert_eq!(
        engine
            .staged
            .facts
            .get(&WindowId("win-a".to_owned()))
            .and_then(|fact| fact.sticky),
        None
    );
    assert_no_state_targets(&engine);
}

#[test]
fn set_sticky_on_from_float_keeps_true_prior() {
    let mut engine = composed_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![cfloat("win-a", "out-a", 0, 0)]),
    );
    assert!(is_floated(&engine, "win-a"));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfloat("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true)
    );
    assert!(is_floated(&engine, "win-a"));
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(true), Some(true))
    );
    assert_eq!(
        engine
            .staged
            .facts
            .get(&WindowId("win-a".to_owned()))
            .and_then(|fact| fact.sticky),
        None
    );
}

#[test]
fn set_sticky_off_false_prior_tiles_with_false_save() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(!is_floated(&engine, "win-a"));
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn set_sticky_off_true_prior_stays_float_with_true_save() {
    let mut engine = composed_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![cfloat("win-a", "out-a", 0, 0)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfloat("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfloat("win-a", "out-a", 0, 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(true), None)
    );
}

#[test]
fn toggle_sticky_flips_both_ways() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            toggle_sticky("win-a"),
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(true), Some(false))
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            toggle_sticky("win-a"),
        ),
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn set_same_state_consumes_without_a_repeat_save() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    // Publish drains the staged save into the reply exactly once.
    assert!(engine.sticky_command_save.is_none(), "drained at publish");
    let before = engine.sticky_saved.clone();
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(
        engine.sticky_command_save.is_none(),
        "same-state stages no save"
    );
    assert_eq!(engine.sticky_saved, before);
    // A consumed duplicate never re-executes either: same command
    // sequence on a fresh observation reconciles without staging.
    match engine.handle_report_legacy(&cmd(
        4,
        2,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-a", true),
    )) {
        LegacyWorldOutcome::CommandConsumed(_) => {}
        other => panic!("expected consumed, got {other:?}"),
    }
    assert!(engine.sticky_command_save.is_none());
    assert_eq!(engine.sticky_saved, before);
}

#[test]
fn toggle_float_clears_sticky_and_tiles_for_both_priors() {
    // Prior false: tiled origin. Prior true: ordinary-float origin
    // (explicit toggle first, so no sticky-independent float origin
    // survives the clear).
    for prior in [false, true] {
        let mut engine = composed_engine();
        accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
        let mut seq = 2;
        let mut cmd_seq = 1;
        if prior {
            accepted(
                &mut engine,
                &cmd(
                    seq,
                    cmd_seq,
                    vec![cfact("win-a", "out-a", 0)],
                    toggle_float("win-a"),
                ),
            );
            assert!(is_floated(&engine, "win-a"));
            seq += 1;
            cmd_seq += 1;
        }
        accepted(
            &mut engine,
            &cmd(
                seq,
                cmd_seq,
                vec![cfact("win-a", "out-a", 0)],
                set_sticky("win-a", true),
            ),
        );
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&prior)
        );
        seq += 1;
        cmd_seq += 1;
        let reply = accepted(
            &mut engine,
            &cmd(
                seq,
                cmd_seq,
                vec![cfact("win-a", "out-a", 0)],
                toggle_float("win-a"),
            ),
        );
        match reply.command_result {
            Some(CommandResult::Float {
                window,
                rect,
                no_space,
            }) => {
                assert_eq!(window, WindowId("win-a".to_owned()));
                assert_eq!(rect, None);
                assert!(!no_space);
            }
            other => panic!("expected tile float result, got {other:?}"),
        }
        assert!(
            !engine
                .restored_sticky
                .contains_key(&WindowId("win-a".to_owned()))
        );
        assert!(is_tiled(&engine, "win-a"), "prior {prior} tiles");
        assert!(!is_floated(&engine, "win-a"), "prior {prior} leaves float");
        assert_eq!(
            staged_save(&engine, "win-a"),
            IntentData::new(Some(false), None)
        );
    }
}

#[test]
fn toggle_float_tiles_a_transient_origin_through_the_explicit_event() {
    // A transient float origin still classifies float after sticky
    // clears, so the clear-and-tile rides the explicit toggle event and
    // lands tiled with the ordinary unfloat vocabulary.
    let mut engine = composed_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![cfloat("win-a", "out-a", 0, 0)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfloat("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true)
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfloat("win-a", "out-a", 0, 0)],
            toggle_float("win-a"),
        ),
    );
    match reply.command_result {
        Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window, WindowId("win-a".to_owned()));
            assert_eq!(rect, None);
            assert!(!no_space);
        }
        other => panic!("expected tile float result, got {other:?}"),
    }
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn sticky_on_lands_on_the_current_workspace() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    let current = engine
        .effective_view(&OutputId("out-a".to_owned()))
        .expect("trailing view");
    assert_ne!(current, "ws-1");
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let id = WindowId("win-a".to_owned());
    assert_eq!(
        engine.members.get(&id).expect("member").home.workspace.0,
        current
    );
    assert_eq!(
        engine.staged_homing(&id).expect("homing").workspace.0,
        current
    );
    assert!(is_floated(&engine, "win-a"));
    // The source leaf is gone: exact single homing, no minimized slot left.
    assert!(!is_tiled(&engine, "win-a"));
}

#[test]
fn sticky_off_to_tile_lands_on_the_current_workspace() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    let current = engine
        .effective_view(&OutputId("out-a".to_owned()))
        .expect("trailing view");
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let id = WindowId("win-a".to_owned());
    assert_eq!(
        engine.members.get(&id).expect("member").home.workspace.0,
        current
    );
    assert_eq!(
        engine.staged_homing(&id).expect("homing").workspace.0,
        current
    );
    assert!(is_tiled(&engine, "win-a"));
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn sticky_off_into_a_floating_workspace_lands_float_without_a_session() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: false,
            },
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    // A floating workspace hosts no session, so float landing shows in
    // membership rather than a session exception.
    assert!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member")
            .floating
    );
    assert!(
        engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    // Base tile intent is saved, but no session is forced until a retiling.
    // The float landing is workspace-driven only: home on the floating
    // scope with no session and no explicit-float mark, so the saved
    // tile restores on retiling instead of pinning the float.
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned())),
        "tile base carries no explicit-float mark"
    );
    let dest = engine
        .staged_homing(&WindowId("win-a".to_owned()))
        .expect("homing");
    assert!(engine.engine.session(&dest).is_none(), "no forced session");
    // Retiling restores the saved tile base.
    accepted(
        &mut engine,
        &cmd(
            5,
            4,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: true,
            },
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            6,
            6,
            vec![screen("out-a", 0)],
            vec![cfact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(is_tiled(&engine, "win-a"), "retile restores the tile base");
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(false), None))
    );
}

#[test]
fn all_six_restore_shapes_turn_off_to_their_exact_prior() {
    let mut engine = composed_engine();
    let shapes: [(Option<bool>, bool); 6] = [
        (None, false),
        (None, true),
        (Some(false), false),
        (Some(false), true),
        (Some(true), false),
        (Some(true), true),
    ];
    let names: Vec<String> = (0..6).map(|i| format!("win-{i}")).collect();
    let records: Vec<IntentRecord> = names
        .iter()
        .zip(shapes)
        .map(|(name, (float, prior))| IntentRecord {
            win: WindowId(name.clone()),
            data: encode_intent_data(&IntentData::new(float, Some(prior))),
        })
        .collect();
    let mut boot = bootstrap(
        names
            .iter()
            .enumerate()
            .map(|(i, name)| cfact(name, "out-a", (i as i32) * 100))
            .collect(),
    );
    boot.intent_records = records;
    // Gapped inventory ingests the whole records pre-admission without
    // settling them away; every command report below carries the full
    // inventory so no sibling is ever implicitly departed.
    boot.inventory_complete = false;
    accepted(&mut engine, &boot);
    let all_facts: Vec<WindowFact> = names
        .iter()
        .enumerate()
        .map(|(i, name)| cfact(name, "out-a", (i as i32) * 100))
        .collect();
    for (index, name) in names.iter().enumerate() {
        let (_, prior) = shapes[index];
        let id = WindowId(name.clone());
        // Pre-command settle may already have committed the whole record
        // into restored provenance on a healthy float admission; either
        // lane carries the exact prior.
        let proven = engine.restored_sticky.get(&id).copied().or_else(|| {
            engine
                .pending_intent
                .get(&id)
                .and_then(|data| data.sticky_prior)
        });
        assert_eq!(proven, Some(prior), "{name} restores whole");
        accepted(
            &mut engine,
            &cmd(
                (index + 2) as u64,
                (index + 1) as u64,
                all_facts.clone(),
                set_sticky(name, false),
            ),
        );
        assert!(!engine.restored_sticky.contains_key(&id), "{name} cleared");
        assert_eq!(
            is_floated(&engine, name),
            prior,
            "{name} lands on its prior"
        );
        assert_eq!(
            is_tiled(&engine, name),
            !prior,
            "{name} tiles only for false prior"
        );
        let save = staged_save(&engine, name);
        assert_eq!(
            save,
            IntentData::new(Some(prior), None),
            "{name} saves its prior"
        );
        // Staged facts stay sticky-null for every restored shape.
        assert_eq!(
            engine.staged.facts.get(&id).and_then(|fact| fact.sticky),
            None,
            "{name} never rewrites facts"
        );
        engine.sticky_command_save.take();
    }
    assert_no_state_targets(&engine);
}

#[test]
fn late_imports_never_clobber_runtime_sticky_knowledge() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let established = IntentData::new(Some(true), Some(false));
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&established)
    );
    engine.sticky_command_save.take();
    let mut late = cmd(
        3,
        2,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-a", false),
    );
    late.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: encode_intent_data(&IntentData::new(Some(false), Some(true))),
    }];
    accepted(&mut engine, &late);
    // The off still honors the established prior, and the late row never
    // clobbers the saved knowledge: it records the new off instead.
    assert!(is_tiled(&engine, "win-a"));
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(false), None))
    );
}

#[test]
fn unsupported_route_consumes_with_state_unsupported() {
    let mut engine =
        WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", unsupported_caps())
            .expect("generation");
    // All sticky routes stay absent here: the null native maximize claim
    // reaches the intended sticky-unsupported gate instead of grading.
    let mut seed = cfact("win-a", "out-a", 0);
    seed.maximized = None;
    accepted(&mut engine, &bootstrap(vec![seed.clone()]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![seed], set_sticky("win-a", true)),
    );
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StateUnsupported
        })
    );
    assert!(engine.restored_sticky.is_empty());
    assert!(engine.sticky_command_save.is_none());
    assert!(engine.sticky_saved.is_empty());
}

#[test]
fn native_route_accepts_sticky_on_with_target() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    let binding = engine
        .retained_states
        .get(&(
            WindowId("win-a".to_owned()),
            crate::world_report::StateProperty::Sticky,
        ))
        .expect("native sticky binding");
    assert!(binding.sticky_meta.is_some());
    assert!(binding.target.value);
    assert_eq!(binding.target.revision, 2);
    assert_eq!(binding.sticky_meta.as_ref().expect("meta").command_seq, 1);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime == WindowId("win-a".to_owned()))
        .expect("published row");
    assert_eq!(row.state.len(), 1);
    assert_eq!(
        row.state[0].property,
        crate::world_report::StateProperty::Sticky
    );
    assert!(row.state[0].value);
    assert_eq!(row.state[0].command_seq, Some(1));
    assert!(engine.sticky_command_save.is_none());
    assert_eq!(reply.intent_writes.len(), 1);
}

#[test]
fn held_scope_refuses_without_mutation() {
    let mut engine = composed_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![
            cfact("win-a", "out-a", 0),
            cfact("win-b", "out-a", 100),
        ]),
    );
    // `win-b` grades to a narrow hold (non-null sticky without native
    // support) while keeping its retained home, so its scope holds and the
    // sibling command must refuse without touching anything.
    let mut held_fact = fact("win-b", "out-a", 100);
    held_fact.sticky = Some(false);
    match engine.handle_report_legacy(&cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0), held_fact],
        set_sticky("win-a", true),
    )) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::ScopeHeld
            }),
            "expected held refusal"
        ),
        other => panic!("expected held refusal, got {other:?}"),
    }
    assert!(engine.restored_sticky.is_empty());
    assert!(engine.sticky_command_save.is_none());
    assert!(!is_floated(&engine, "win-a"));
}

#[test]
fn minimized_and_excluded_subjects_refuse_without_mutation() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    let mut minimized = cfact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    match engine.handle_report_legacy(&cmd(2, 1, vec![minimized], set_sticky("win-a", true))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected refusal"
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    let mut excluded = cfact("win-a", "out-a", 0);
    excluded.privileged = Some(true);
    match engine.handle_report_legacy(&cmd(3, 2, vec![excluded], set_sticky("win-a", true))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected refusal"
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert!(engine.restored_sticky.is_empty());
    assert!(engine.sticky_command_save.is_none());
}

#[test]
fn unknown_window_refuses_without_mutation() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    match engine.handle_report_legacy(&cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-ghost", true),
    )) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected refusal"
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert!(engine.restored_sticky.is_empty());
    assert!(engine.sticky_command_save.is_none());
}

#[test]
fn sticky_on_publishes_once_then_commandless_empty() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(reply.intent_writes.len(), 1);
    assert_eq!(
        decode_intent_data(reply.intent_writes[0].data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(true), Some(false))
    );
    assert!(engine.sticky_command_save.is_none(), "drained exactly once");
    let idle = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cfact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(idle.intent_writes.is_empty(), "no leak into later replies");
    assert!(engine.sticky_command_save.is_none());
}

#[test]
fn sticky_toggle_float_clear_publishes_tile_save() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            toggle_float("win-a"),
        ),
    );
    assert_eq!(reply.intent_writes.len(), 1);
    assert_eq!(
        decode_intent_data(reply.intent_writes[0].data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(false), None)
    );
    assert!(engine.sticky_command_save.is_none());
}

#[test]
fn duplicate_repeats_save_while_consumed_emits_none() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    let fresh = cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        toggle_sticky("win-a"),
    );
    let reply = accepted(&mut engine, &fresh);
    assert_eq!(reply.intent_writes.len(), 1);
    match engine.handle_report_legacy(&fresh) {
        LegacyWorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.intent_writes, reply.intent_writes)
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
    match engine.handle_report_legacy(&cmd(
        3,
        1,
        vec![cfact("win-a", "out-a", 0)],
        toggle_sticky("win-a"),
    )) {
        LegacyWorldOutcome::CommandConsumed(consumed) => {
            assert!(consumed.intent_writes.is_empty(), "consumed emits none")
        }
        other => panic!("expected consumed, got {other:?}"),
    }
    assert!(engine.sticky_command_save.is_none(), "consumed cannot leak");
}

#[test]
fn ordinary_toggle_float_publishes_and_upserts_saved_knowledge() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    assert!(engine.sticky_saved.is_empty());
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            toggle_float("win-a"),
        ),
    );
    assert_eq!(reply.intent_writes.len(), 1);
    assert_eq!(
        decode_intent_data(reply.intent_writes[0].data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(true), None)
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(true), None))
    );
}

// --- Composed sticky visibility lane: stable `sticky_visible_rev`
// decoration over the assembled publish rows. ---

fn sup_fact(window: &str, output: &str, x: i32) -> WindowFact {
    let mut row = cfact(window, output, x);
    row.suppression_available = Some(true);
    row.visibility.suppression_owned = Some(false);
    row
}

fn sup_float(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
    let mut row = cfloat(window, output, x, y);
    row.suppression_available = Some(true);
    row.visibility.suppression_owned = Some(false);
    row
}

fn desire<'a>(reply: &'a crate::world_report::WorldReply, win: &str) -> &'a WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime == WindowId(win.to_owned()))
        .unwrap_or_else(|| panic!("row for {win}"))
}

fn assert_fact_sticky_none(engine: &WorldEngine, win: &str) {
    assert_eq!(
        engine
            .staged
            .facts
            .get(&WindowId(win.to_owned()))
            .and_then(|fact| fact.sticky),
        None,
        "{win} never rewrites raw sticky facts"
    );
}

#[test]
fn sticky_on_decorates_visibility_with_reply_rev() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![sup_fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let row = desire(&reply, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: reply.state_revision,
            fallback: false,
        })
    );
    assert_eq!(row.visibility_authority, ScopeAuthority::Controlled);
    assert!(row.state.is_empty(), "no state writes on this lane");
    assert_eq!(
        engine.sticky_visible_rev.get(&WindowId("win-a".to_owned())),
        Some(&reply.state_revision)
    );
    assert_fact_sticky_none(&engine, "win-a");
    // The actual reply encoding carries the composed target: the sizing
    // backstop protects the wire while this intermediate shape lands.
    let wire = encode_reply_canonical(&reply);
    assert!(
        wire.contains(&format!(
            "\"visibility\":{{\"shown\":true,\"rev\":\"{}\",\"fallback\":false}}",
            reply.state_revision
        )),
        "encoded composed show target"
    );
}

#[test]
fn view_change_without_a_save_keeps_visibility_rev_stable() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![sup_fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = reply.state_revision;
    assert_eq!(
        desire(&reply, "win-a").visibility_authority,
        ScopeAuthority::Controlled
    );
    // Record-free view change: no save may emit and the home must not
    // move the sticky window off its output.
    let moved = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![sup_fact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    assert!(moved.intent_writes.is_empty(), "view change stages no save");
    assert_eq!(
        engine.sticky_visible_rev.get(&WindowId("win-a".to_owned())),
        Some(&rev),
        "observation never renews the stable rev"
    );
    let row = desire(&moved, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    assert_eq!(row.visibility_authority, ScopeAuthority::Controlled);
    assert_eq!(row.output, OutputId("out-a".to_owned()));
    assert!(row.state.is_empty());
    assert_fact_sticky_none(&engine, "win-a");
}

#[test]
fn suppression_hold_then_recovery_keeps_the_same_rev() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![sup_fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = reply.state_revision;
    let mut denied = sup_fact("win-a", "out-a", 0);
    denied.suppression_available = Some(false);
    let held = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![denied],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(held.intent_writes.is_empty());
    let row = desire(&held, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        }),
        "gated rows retain the stable target"
    );
    assert_ne!(
        row.visibility_authority,
        ScopeAuthority::Controlled,
        "suppression false never controls"
    );
    let mut unknown = sup_fact("win-a", "out-a", 0);
    unknown.suppression_available = None;
    let unknown_reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![unknown],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_ne!(
        desire(&unknown_reply, "win-a").visibility_authority,
        ScopeAuthority::Controlled,
        "suppression null never controls"
    );
    assert_eq!(
        desire(&unknown_reply, "win-a").visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    let recovered = accepted(
        &mut engine,
        &report_full(
            5,
            5,
            vec![screen("out-a", 0)],
            vec![sup_fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let row = desire(&recovered, "win-a");
    assert_eq!(row.visibility_authority, ScopeAuthority::Controlled);
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        }),
        "recovery reuses the same stable rev"
    );
    assert_eq!(
        engine.sticky_visible_rev.get(&WindowId("win-a".to_owned())),
        Some(&rev)
    );
    assert_fact_sticky_none(&engine, "win-a");
    assert!(row.state.is_empty());
}

#[test]
fn gated_windows_never_control_visibility() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![sup_fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = reply.state_revision;
    // Unknown maximized never defaults false: held, same stable target.
    let mut unknown_max = sup_fact("win-a", "out-a", 0);
    unknown_max.maximized = None;
    let gated = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![unknown_max],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_ne!(
        desire(&gated, "win-a").visibility_authority,
        ScopeAuthority::Controlled
    );
    assert_eq!(
        desire(&gated, "win-a").visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    // Minimized never controls.
    let mut minimized = sup_fact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    let gated = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![minimized],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_ne!(
        desire(&gated, "win-a").visibility_authority,
        ScopeAuthority::Controlled
    );
    assert_eq!(
        desire(&gated, "win-a").visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    // Gesture barrier never controls.
    let mut barrier = report_full(
        5,
        5,
        vec![screen("out-a", 0)],
        vec![sup_fact("win-a", "out-a", 0)],
        vec![],
        "",
        None,
        None,
    );
    barrier.gesture_barrier = true;
    let gated = accepted(&mut engine, &barrier);
    let row = desire(&gated, "win-a");
    assert_ne!(row.visibility_authority, ScopeAuthority::Controlled);
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    assert!(row.state.is_empty());
    // Held scope (retained sibling turns unreadable, holding the shared
    // scope) never controls.
    accepted(
        &mut engine,
        &report_full(
            6,
            6,
            vec![screen("out-a", 0)],
            vec![
                sup_fact("win-a", "out-a", 0),
                sup_fact("win-b", "out-a", 100),
            ],
            vec![],
            "",
            None,
            None,
        ),
    );
    let mut unreadable_sibling = sup_fact("win-b", "out-a", 100);
    unreadable_sibling.rect = None;
    let gated = accepted(
        &mut engine,
        &report_full(
            7,
            7,
            vec![screen("out-a", 0)],
            vec![sup_fact("win-a", "out-a", 0), unreadable_sibling],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_ne!(
        desire(&gated, "win-a").visibility_authority,
        ScopeAuthority::Controlled
    );
    assert_eq!(
        desire(&gated, "win-a").visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    assert_eq!(
        engine.sticky_visible_rev.get(&WindowId("win-a".to_owned())),
        Some(&rev)
    );
    assert_fact_sticky_none(&engine, "win-a");
}

#[test]
fn visibility_stays_on_the_owning_output_only() {
    let mut engine = composed_engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![
                sup_fact("win-a", "out-a", 0),
                sup_fact("win-b", "out-b", 800),
            ],
            vec![],
            "",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![
                sup_fact("win-a", "out-a", 0),
                sup_fact("win-b", "out-b", 800),
            ],
            vec![],
            "",
            Some(1),
            Some(set_sticky("win-a", true)),
        ),
    );
    let sticky_row = desire(&reply, "win-a");
    assert_eq!(sticky_row.output, OutputId("out-a".to_owned()));
    assert_eq!(
        sticky_row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: reply.state_revision,
            fallback: false,
        })
    );
    assert_eq!(sticky_row.visibility_authority, ScopeAuthority::Controlled);
    let plain_row = desire(&reply, "win-b");
    assert_eq!(plain_row.visibility, None, "ordinary rows stay untouched");
    assert_eq!(plain_row.visibility_authority, ScopeAuthority::Untouched);
    assert!(
        reply
            .windows
            .iter()
            .filter(|row| row.lifetime == WindowId("win-a".to_owned()))
            .all(|row| row.output == OutputId("out-a".to_owned())),
        "sticky target never leaks onto the second output"
    );
    assert!(sticky_row.state.is_empty() && plain_row.state.is_empty());
    assert_fact_sticky_none(&engine, "win-a");
    assert_fact_sticky_none(&engine, "win-b");
}

#[test]
fn restore_admits_a_visibility_target_without_a_save() {
    let mut engine = composed_engine();
    let mut boot = bootstrap(vec![sup_float("win-a", "out-a", 0, 0)]);
    boot.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: encode_intent_data(&IntentData::new(None, Some(true))),
    }];
    let reply = accepted(&mut engine, &boot);
    assert!(reply.intent_writes.is_empty(), "restore stages no save");
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true)
    );
    assert_eq!(
        engine.sticky_visible_rev.get(&WindowId("win-a".to_owned())),
        Some(&reply.state_revision),
        "first safe admission initializes the stable rev"
    );
    let row = desire(&reply, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: reply.state_revision,
            fallback: false,
        })
    );
    assert_eq!(row.visibility_authority, ScopeAuthority::Controlled);
    assert!(row.state.is_empty());
    assert_fact_sticky_none(&engine, "win-a");
}

#[test]
fn sticky_reply_bound_rejects_before_mutation_and_small_retry_wins() {
    // Pre-mutation REPLY bound refusal driven by the new sticky reserves
    // (the save row plus the composed-show lanes), not the request-size
    // gate: the report itself fits, but the worst-case reply does not. The
    // padding is tuned so the bound WITHOUT the new reserves still fits
    // while the bound WITH them exceeds it, proving the new reserves are
    // the discriminator. The engine refuses before any mutation and the
    // same command sequence still wins once the report fits.
    use crate::world_engine::KIND_REPLY_SIZE;
    use crate::world_report::{WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES};
    use crate::world_sizing::{
        MAX_TARGET_LANES_PER_WINDOW, json_string_len, max_intent_save_bound, reply_upper_bound,
        report_estimate,
    };
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let logical = engine.engine.domain_keys().len();
    let staged = engine.staged.facts.len() + engine.staged.held_windows.len();
    let retained = engine.retained_publish_footprint();
    let save_bound = max_intent_save_bound(&cmd(2, 1, Vec::new(), set_sticky("win-a", true)));
    assert_eq!(save_bound, 16 + json_string_len("win-a") + 48);
    let lanes_of = |windows_len: usize, held_len: usize| {
        windows_len
            .saturating_add(held_len)
            .saturating_add(staged)
            .saturating_add(retained)
            .saturating_mul(MAX_TARGET_LANES_PER_WINDOW)
    };
    let report_of = |windows: Vec<WindowFact>| cmd(2, 1, windows, set_sticky("win-a", true));
    let bound_of = |windows: &[WindowFact]| {
        reply_upper_bound(
            &report_of(windows.to_vec()),
            0,
            logical,
            1,
            staged,
            retained,
        )
    };
    let new_terms =
        |windows_len: usize, held_len: usize| save_bound + lanes_of(windows_len, held_len);
    // Coarse: pad until the bound without the new sticky reserves fits.
    let mut windows = vec![sup_fact("win-a", "out-a", 0), cfact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(cfact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) - new_terms(windows.len(), 0) > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    // Fine: grow one bounded title on an existing pad window - each ASCII
    // title byte adds exactly one to estimate and bound alike (plus 2 for
    // the null-to-string swap) - landing the reserveless bound within one
    // combined reserve below the cap.
    let reserve = new_terms(windows.len(), 0);
    assert!(reserve > 200, "combined reserve dominates: {reserve}");
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(200)
        .saturating_sub(bound_of(&windows) - reserve)
        + 2;
    windows[2].title = Some("t".repeat(deficit));
    let report = report_of(windows);
    let terms = new_terms(report.windows.len(), report.held_windows.len());
    let without = reply_upper_bound(&report, 0, logical, 1, staged, retained) - terms;
    assert!(
        report_estimate(&report) <= WORLD_MAX_REQUEST_BYTES,
        "request itself fits; only the reply bound trips"
    );
    assert!(
        without <= WORLD_MAX_REPLY_BYTES,
        "reserveless bound fits: {without}"
    );
    assert!(
        without + terms > WORLD_MAX_REPLY_BYTES,
        "sticky reserves are the discriminator: {without} + {terms}"
    );
    let revision = engine.state_revision;
    let sticky = engine.restored_sticky.clone();
    let saved = engine.sticky_saved.clone();
    match engine.handle_report_legacy(&report) {
        LegacyWorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, KIND_REPLY_SIZE);
        }
        other => panic!("expected reply-oversize, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision, "refusal mutates nothing");
    assert!(engine.command_highwater.is_none(), "command unconsumed");
    assert_eq!(engine.restored_sticky, sticky, "provenance intact");
    assert_eq!(engine.sticky_saved, saved, "no staged save leaks");
    // The same command sequence still wins once the report fits: the small
    // retry publishes the sticky save and the composed show inside the
    // pre-mutation bound.
    let retry = cmd(
        2,
        1,
        vec![sup_fact("win-a", "out-a", 0)],
        set_sticky("win-a", true),
    );
    let pre_bound = reply_upper_bound(&retry, 0, logical, 1, staged, retained);
    let reply = accepted(&mut engine, &retry);
    assert_eq!(
        decode_intent_data(reply.intent_writes[0].data.as_deref().expect("save"))
            .expect("save decodes"),
        IntentData::new(Some(true), Some(false))
    );
    let row = desire(&reply, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: reply.state_revision,
            fallback: false,
        })
    );
    assert_eq!(row.visibility_authority, ScopeAuthority::Controlled);
    assert!(row.state.is_empty());
    let actual = encode_reply_canonical(&reply);
    assert!(
        actual.len() <= pre_bound,
        "retry reply {} fits pre-bound {pre_bound}",
        actual.len()
    );
}

#[test]
fn toggle_float_clears_sticky_on_the_current_workspace_for_both_priors() {
    for prior in [false, true] {
        let mut engine = composed_engine();
        accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
        let mut seq = 2;
        let mut cmd_seq = 1;
        if prior {
            accepted(
                &mut engine,
                &cmd(
                    seq,
                    cmd_seq,
                    vec![cfact("win-a", "out-a", 0)],
                    toggle_float("win-a"),
                ),
            );
            seq += 1;
            cmd_seq += 1;
        }
        accepted(
            &mut engine,
            &cmd(
                seq,
                cmd_seq,
                vec![cfact("win-a", "out-a", 0)],
                set_sticky("win-a", true),
            ),
        );
        seq += 1;
        cmd_seq += 1;
        accepted(
            &mut engine,
            &cmd(
                seq,
                cmd_seq,
                vec![cfact("win-a", "out-a", 0)],
                WorldCommand::SelectTrailing { output: None },
            ),
        );
        let current = engine
            .effective_view(&OutputId("out-a".to_owned()))
            .expect("trailing view");
        seq += 1;
        cmd_seq += 1;
        let reply = accepted(
            &mut engine,
            &cmd(
                seq,
                cmd_seq,
                vec![cfact("win-a", "out-a", 0)],
                toggle_float("win-a"),
            ),
        );
        match reply.command_result {
            Some(CommandResult::Float {
                window,
                rect,
                no_space,
            }) => {
                assert_eq!(window, WindowId("win-a".to_owned()));
                assert_eq!(rect, None);
                assert!(!no_space);
            }
            other => panic!("prior {prior}: expected tile float result, got {other:?}"),
        }
        let id = WindowId("win-a".to_owned());
        assert_eq!(
            engine.members.get(&id).expect("member").home.workspace.0,
            current,
            "prior {prior} tiles at the current workspace"
        );
        assert_eq!(
            engine.staged_homing(&id).expect("homing").workspace.0,
            current,
            "prior {prior} homing follows the current workspace"
        );
        assert!(is_tiled(&engine, "win-a"), "prior {prior} tiles");
        assert!(!is_floated(&engine, "win-a"), "prior {prior} leaves float");
        assert_eq!(
            staged_save(&engine, "win-a"),
            IntentData::new(Some(false), None),
            "prior {prior} saves the tile base"
        );
        // Source cleanup: no stale minimized slot survives the move and no
        // sticky provenance leaks.
        assert!(
            !engine
                .restored_sticky
                .contains_key(&WindowId("win-a".to_owned())),
            "prior {prior} clears sticky"
        );
    }
}

#[test]
fn toggle_float_from_a_floating_source_still_tiles_the_current_workspace() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    let current = engine
        .effective_view(&OutputId("out-a".to_owned()))
        .expect("trailing view");
    // Float the source workspace after the sticky move: the blanket source
    // refusal must not fire; mode policy on the destination still tiles.
    accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: false,
            },
        ),
    );
    assert!(
        engine
            .floating_scopes
            .iter()
            .any(|key| key.workspace.0 == "ws-1")
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            5,
            4,
            vec![cfact("win-a", "out-a", 0)],
            toggle_float("win-a"),
        ),
    );
    match reply.command_result {
        Some(CommandResult::Float {
            rect: None,
            no_space: false,
            ..
        }) => {}
        other => panic!("expected tile float result, got {other:?}"),
    }
    let id = WindowId("win-a".to_owned());
    assert_eq!(
        engine.members.get(&id).expect("member").home.workspace.0,
        current
    );
    assert!(is_tiled(&engine, "win-a"));
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn toggle_float_into_a_floating_destination_lands_float_with_tile_base() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    let current = engine
        .effective_view(&OutputId("out-a".to_owned()))
        .expect("trailing view");
    accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: current.clone(),
                tiled: false,
            },
        ),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            5,
            4,
            vec![cfact("win-a", "out-a", 0)],
            toggle_float("win-a"),
        ),
    );
    match reply.command_result {
        Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window, WindowId("win-a".to_owned()));
            assert_eq!(rect, None);
            assert!(!no_space);
        }
        other => panic!("expected float landing result, got {other:?}"),
    }
    let id = WindowId("win-a".to_owned());
    assert_eq!(
        engine.members.get(&id).expect("member").home.workspace.0,
        current
    );
    // Workspace-driven float landing: home on the floating scope with no
    // session, no explicit-float mark, and the tile base saved.
    assert!(
        !engine.restored_floats.contains(&id),
        "tile base carries no explicit-float mark"
    );
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None),
        "base tile intent is saved under the float landing"
    );
    let dest = engine.staged_homing(&id).expect("homing");
    assert!(engine.engine.session(&dest).is_none(), "no forced session");
    // Retiling restores the saved tile base.
    accepted(
        &mut engine,
        &cmd(
            6,
            5,
            vec![cfact("win-a", "out-a", 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: current.clone(),
                tiled: true,
            },
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            7,
            7,
            vec![screen("out-a", 0)],
            vec![cfact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(is_tiled(&engine, "win-a"), "retile restores the tile base");
    assert!(!engine.restored_floats.contains(&id));
    assert_eq!(
        engine.sticky_saved.get(&id),
        Some(&IntentData::new(Some(false), None))
    );
}

#[test]
fn set_sticky_off_prior_false_with_transient_origin_tiles_via_explicit() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    // The raw origin turns transient while sticky: convergence alone keeps
    // classifying float, so off-to-tile must ride the explicit event rather
    // than refuse.
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfloat("win-a", "out-a", 0, 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(
        reply.command_result.is_none(),
        "off carries no float result"
    );
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(is_tiled(&engine, "win-a"), "transient origin still tiles");
    assert!(!is_floated(&engine, "win-a"));
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn sticky_on_refuses_when_the_joint_ledger_is_full_without_mutation() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    for i in 0..crate::world_report::MAX_INTENT_RECORDS {
        engine.pending_intent.insert(
            WindowId(format!("win-fill-{i:05}")),
            IntentData::new(Some(false), None),
        );
    }
    assert_eq!(
        engine.intent_ledger_used(),
        crate::world_report::MAX_INTENT_RECORDS
    );
    let revision = engine.state_revision;
    let sticky = engine.restored_sticky.clone();
    let saved = engine.sticky_saved.clone();
    match engine.handle_report_legacy(&cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-a", true),
    )) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected joint-cap refusal"
        ),
        other => panic!("expected joint-cap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert_eq!(engine.command_highwater, Some(1), "command consumed");
    assert_eq!(engine.restored_sticky, sticky, "no provenance leaks");
    assert_eq!(engine.sticky_saved, saved, "no save leaks");
    assert!(engine.sticky_command_save.is_none(), "no staged save leaks");
}

#[test]
fn sticky_on_refuses_when_saved_knowledge_is_full_without_mutation() {
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    for i in 0..crate::world_report::MAX_INTENT_RECORDS {
        engine.sticky_saved.insert(
            WindowId(format!("win-saved-{i:05}")),
            IntentData::new(Some(false), None),
        );
    }
    let revision = engine.state_revision;
    let sticky = engine.restored_sticky.clone();
    let refused_obs = cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-a", true),
    );
    // Commandless control with the same observation: the refusal must
    // converge identically apart from the consumed sequence and typed
    // result. Synthetic ledger fillers are not observation-live, so the
    // adoption path legitimately prunes them on either path.
    let mut control = engine.clone();
    let mut control_obs = refused_obs.clone();
    control_obs.command = None;
    control_obs.command_seq = None;
    let _ = control.handle_report_legacy(&control_obs);
    match engine.handle_report_legacy(&refused_obs) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected saved-cap refusal"
        ),
        other => panic!("expected saved-cap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert_eq!(engine.command_highwater, Some(1), "command consumed");
    assert_eq!(engine.restored_sticky, sticky, "no provenance leaks");
    assert_eq!(
        engine.sticky_saved, control.sticky_saved,
        "saved knowledge converges with the commandless control"
    );
    assert!(engine.sticky_command_save.is_none(), "no staged save leaks");
}

#[test]
fn visibility_unsupported_maximize_null_stays_controlled() {
    // Contract-valid absent `maximized: null` with `maximize: false` must
    // not hold the composed show.
    let caps = HostCapabilities {
        native_sticky: false,
        window_suppression: true,
        maximize: false,
        ..Default::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps)
        .expect("generation");
    let mut row = sup_fact("win-a", "out-a", 0);
    row.maximized = None;
    accepted(&mut engine, &bootstrap(vec![row.clone()]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![row], set_sticky("win-a", true)),
    );
    let shown = desire(&reply, "win-a");
    assert_eq!(
        shown.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: reply.state_revision,
            fallback: false,
        })
    );
    assert_eq!(shown.visibility_authority, ScopeAuthority::Controlled);
}

#[test]
fn visibility_supported_maximize_null_holds_frame_only() {
    // Supported-but-null `maximized` holds the dependent lane: the stable
    // target is retained with a non-Controlled authority.
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![sup_fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = reply.state_revision;
    assert_eq!(
        desire(&reply, "win-a").visibility_authority,
        ScopeAuthority::Controlled
    );
    let mut unknown_max = sup_fact("win-a", "out-a", 0);
    unknown_max.maximized = None;
    let gated = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![unknown_max],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(gated.intent_writes.is_empty());
    let row = desire(&gated, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        }),
        "gated rows retain the stable target"
    );
    assert_ne!(
        row.visibility_authority,
        ScopeAuthority::Controlled,
        "supported null never controls"
    );
}

#[test]
fn visibility_owned_unavailable_null_stays_controlled() {
    // `owned` listed unavailable skips the gate: null must not hold.
    let caps = HostCapabilities {
        native_sticky: false,
        window_suppression: true,
        maximize: true,
        unavailable_facts: vec![crate::connection::FactName::Owned],
        ..Default::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps)
        .expect("generation");
    let mut row = sup_fact("win-a", "out-a", 0);
    row.owned = None;
    accepted(&mut engine, &bootstrap(vec![row.clone()]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![row], set_sticky("win-a", true)),
    );
    let shown = desire(&reply, "win-a");
    assert_eq!(
        shown.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: reply.state_revision,
            fallback: false,
        })
    );
    assert_eq!(shown.visibility_authority, ScopeAuthority::Controlled);
}

#[test]
fn visibility_owned_supported_null_holds_frame_only() {
    // Supported-but-null `owned` holds: same stable target, no control.
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![sup_fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![sup_fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = reply.state_revision;
    let mut unknown_owned = sup_fact("win-a", "out-a", 0);
    unknown_owned.owned = None;
    let gated = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![unknown_owned],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(gated.intent_writes.is_empty());
    let row = desire(&gated, "win-a");
    assert_eq!(
        row.visibility,
        Some(VisibilityTarget {
            shown: true,
            revision: rev,
            fallback: false,
        })
    );
    assert_ne!(
        row.visibility_authority,
        ScopeAuthority::Controlled,
        "supported owned-null never controls"
    );
}

#[test]
fn sticky_on_replaces_superseded_block_at_exact_cap() {
    // A superseded save block is removed on candidate success before the
    // joint cap check, so block-to-provenance transfer at exactly MAX
    // holds steady instead of refusing.
    use crate::world_report::MAX_INTENT_RECORDS;
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    engine
        .intent_sticky_blocks
        .insert(WindowId("win-a".to_owned()));
    for i in 0..(MAX_INTENT_RECORDS - 1) {
        engine.pending_intent.insert(
            WindowId(format!("win-fill-{i:05}")),
            IntentData::new(Some(false), None),
        );
    }
    assert_eq!(engine.intent_ledger_used(), MAX_INTENT_RECORDS);
    // Incomplete inventory keeps the filler lifetimes live (no proven
    // departure prunes them) while the sticky transition still runs.
    let mut command = cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-a", true),
    );
    command.inventory_complete = false;
    let reply = accepted(&mut engine, &command);
    assert!(reply.command_result.is_none());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "superseded block removed"
    );
    assert_eq!(
        engine.intent_ledger_used(),
        MAX_INTENT_RECORDS,
        "transfer holds the joint count steady"
    );
    assert!(!engine.intent_sticky_saturated, "latch untouched");
    assert_eq!(
        staged_save(&engine, "win-a"),
        IntentData::new(Some(true), Some(false))
    );
}

#[test]
fn sticky_clear_refuses_whole_candidate_when_no_space_and_smaller_retry_wins() {
    // Tiny zero-gap exhaustion, same geometry as the ordinary
    // `no_space_retry_emits_no_save_and_stays_float` witness: the seed
    // capacity-floats one window; sticky-on adopts that float, and the
    // sticky clear rides the explicit `ToggleFloat` event (transient
    // echo) which re-floats for lack of space. The no-space outcome
    // refuses the whole candidate with no false tile save; the same
    // command retries smaller (one filler closed) and tiles with its
    // save.
    use crate::connection::RawSettingValue;
    use crate::geometry::Rect;
    let engine_of = || {
        WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            composed_caps(),
            vec![("outer".to_owned(), RawSettingValue::Number(0.0))],
        )
        .expect("generation")
    };
    let tiny = || crate::world_report::ScreenReport {
        output: OutputId("out-a".to_owned()),
        full_rect: Some(Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        }),
        work_area: Some(Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        }),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    };
    let tiny_report = |seq: u64, windows: Vec<WindowFact>| {
        report_full(seq, seq, vec![tiny()], windows, vec![], "", None, None)
    };
    let tiny_cmd = |seq: u64, cseq: u64, windows: Vec<WindowFact>, command: WorldCommand| {
        report_full(
            seq,
            seq,
            vec![tiny()],
            windows,
            vec![],
            "",
            Some(cseq),
            Some(command),
        )
    };
    let mut engine = engine_of();
    let seed: Vec<WindowFact> = (0..5)
        .map(|i| cfact(&format!("w{i}"), "out-a", i * 10))
        .collect();
    let first = accepted(&mut engine, &tiny_report(1, seed.clone()));
    let victim = first
        .windows
        .iter()
        .find(|row| row.floating)
        .map(|row| row.lifetime.0.clone())
        .expect("exhaustion floats one window");
    accepted(
        &mut engine,
        &tiny_cmd(2, 1, seed.clone(), set_sticky(&victim, true)),
    );
    assert!(is_floated(&engine, &victim));
    assert_eq!(
        engine.restored_sticky.get(&WindowId(victim.clone())),
        Some(&true),
        "sticky adopts the capacity float"
    );
    // The clear echoes the floated observation, so it rides the explicit
    // event exactly like the ordinary witness.
    let echo: Vec<WindowFact> = seed
        .iter()
        .map(|w| {
            let mut w = w.clone();
            if w.window.0 == victim {
                w.transient = Some(true);
            }
            w
        })
        .collect();
    let revision = engine.state_revision;
    let highwater = engine.command_highwater;
    let sticky = engine.restored_sticky.clone();
    let saved = engine.sticky_saved.clone();
    match engine.handle_report_legacy(&tiny_cmd(3, 2, echo.clone(), toggle_float(&victim))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected no-space refusal"
        ),
        other => panic!("expected no-space refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert_eq!(
        engine.command_highwater,
        Some(2),
        "refused sequence still consumes"
    );
    assert_eq!(highwater, Some(1), "sticky-on consumed its own sequence");
    assert_eq!(engine.restored_sticky, sticky, "sticky retained");
    assert_eq!(engine.sticky_saved, saved, "no false save staged");
    assert!(engine.sticky_command_save.is_none(), "no staged save leaks");
    assert!(is_floated(&engine, &victim), "victim stays floated");
    assert!(!is_tiled(&engine, &victim), "never retiled by refusal");
    // Smaller retry: the closed filler frees one tile slot; the same
    // command sequence tiles with its save.
    let dropped = seed
        .iter()
        .map(|w| w.window.0.clone())
        .find(|name| *name != victim)
        .expect("a filler to close");
    let smaller: Vec<WindowFact> = echo.into_iter().filter(|w| w.window.0 != dropped).collect();
    let retry = accepted(&mut engine, &tiny_cmd(4, 3, smaller, toggle_float(&victim)));
    match retry.command_result {
        Some(CommandResult::Float {
            rect: None,
            no_space: false,
            ..
        }) => {}
        other => panic!("expected tile float result, got {other:?}"),
    }
    assert!(is_tiled(&engine, &victim));
    assert_eq!(
        decode_intent_data(retry.intent_writes[0].data.as_deref().expect("save"))
            .expect("save decodes"),
        IntentData::new(Some(false), None)
    );
}

#[test]
fn null_null_import_caches_exact_saved_without_pending_authority() {
    let mut engine = composed_engine();
    let data = encode_intent_data(&IntentData::new(None, None));
    let mut boot = bootstrap(vec![cfact("win-n", "out-a", 0)]);
    boot.intent_records = vec![IntentRecord {
        win: WindowId("win-n".to_owned()),
        data,
    }];
    boot.inventory_complete = false;
    accepted(&mut engine, &boot);
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-n".to_owned())),
        Some(&IntentData::new(None, None)),
        "null/null exact shape cached"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-n".to_owned())),
        "null/null confers no restore authority"
    );
    assert!(
        !engine
            .intent_conflicts
            .contains(&WindowId("win-n".to_owned())),
        "null/null is not a conflict"
    );
    assert!(
        !engine.sticky_saved_incomplete,
        "null/null hit never latches"
    );
}

#[test]
fn conflicting_import_drops_bogus_first_saved_and_latches() {
    let mut engine = composed_engine();
    let first = encode_intent_data(&IntentData::new(Some(false), Some(false)));
    let mut boot = bootstrap(vec![cfact("win-a", "out-a", 0)]);
    boot.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: first,
    }];
    boot.inventory_complete = false;
    accepted(&mut engine, &boot);
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(false), Some(false)))
    );
    let second = encode_intent_data(&IntentData::new(Some(true), Some(true)));
    let mut clash = bootstrap(vec![cfact("win-a", "out-a", 0)]);
    clash.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: second,
    }];
    clash.inventory_complete = false;
    accepted(&mut engine, &clash);
    assert!(
        !engine
            .sticky_saved
            .contains_key(&WindowId("win-a".to_owned())),
        "conflict must not leave bogus first prior"
    );
    assert!(
        engine
            .intent_conflicts
            .contains(&WindowId("win-a".to_owned())),
        "mismatch marks conflict"
    );
    assert!(
        engine.sticky_saved_incomplete,
        "conflicting preimage latches incomplete"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned())),
        "conflict drops pending"
    );
}

#[test]
fn saved_full_import_latches_and_native_still_refuses_after_prune() {
    use crate::world_report::MAX_INTENT_RECORDS;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &bootstrap(vec![
            cfact("win-a", "out-a", 0),
            cfact("win-new", "out-a", 100),
        ]),
    );
    for i in 0..MAX_INTENT_RECORDS {
        engine.sticky_saved.insert(
            WindowId(format!("win-fill-{i:05}")),
            IntentData::new(Some(false), None),
        );
    }
    assert!(!engine.sticky_saved_incomplete);
    let data = encode_intent_data(&IntentData::new(Some(false), Some(false)));
    let mut full = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![cfact("win-a", "out-a", 0), cfact("win-new", "out-a", 100)],
        vec![],
        "",
        None,
        None,
    );
    full.intent_records = vec![IntentRecord {
        win: WindowId("win-new".to_owned()),
        data,
    }];
    full.inventory_complete = false;
    accepted(&mut engine, &full);
    assert!(
        engine.sticky_saved_incomplete,
        "saved cap drop latches incomplete"
    );
    engine
        .sticky_saved
        .remove(&WindowId("win-fill-00000".to_owned()));
    assert!(
        engine.sticky_saved.len() < MAX_INTENT_RECORDS,
        "prune frees room"
    );
    assert!(engine.sticky_saved_incomplete, "latch survives prune");
    // Fail-closed: a supplied `Some(false)` raw proves the refusal comes
    // from the latched incomplete prior (the knowledge gate), not an
    // unreadable null: native observations stay authoritative, so the
    // known non-sticky fact reaches the exact-prior check and refuses
    // through the typed operation lane with no save and no binding.
    match engine.handle_report_legacy(&cmd(
        3,
        1,
        vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 100)],
        set_sticky("win-new", false),
    )) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected knowledge-gate refusal"
        ),
        other => panic!("expected knowledge-gate refusal, got {other:?}"),
    }
    assert!(engine.sticky_command_save.is_none());
    assert!(engine.retained_states.is_empty());
}

#[test]
fn ordinary_save_overflow_latches_and_late_ignored_never_latches() {
    use crate::world_report::MAX_INTENT_RECORDS;
    let mut engine = composed_engine();
    accepted(&mut engine, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    for i in 0..MAX_INTENT_RECORDS {
        engine.sticky_saved.insert(
            WindowId(format!("win-fill-{i:05}")),
            IntentData::new(Some(false), None),
        );
    }
    assert!(!engine.sticky_saved_incomplete);
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            toggle_float("win-a"),
        ),
    );
    assert_eq!(reply.intent_writes.len(), 1, "ordinary float still emits");
    assert!(
        engine.sticky_saved_incomplete,
        "ordinary save cap drop latches"
    );
    assert!(
        !engine
            .sticky_saved
            .contains_key(&WindowId("win-a".to_owned())),
        "overflowed save is not silently kept"
    );
    let mut engine2 = composed_engine();
    accepted(&mut engine2, &bootstrap(vec![cfact("win-a", "out-a", 0)]));
    accepted(
        &mut engine2,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(!engine2.sticky_saved_incomplete);
    let mut late = cmd(
        3,
        2,
        vec![cfact("win-a", "out-a", 0)],
        set_sticky("win-a", false),
    );
    late.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: encode_intent_data(&IntentData::new(Some(false), Some(true))),
    }];
    accepted(&mut engine2, &late);
    assert!(
        !engine2.sticky_saved_incomplete,
        "late ignored input never latches"
    );
}

#[test]
fn null_null_first_then_valid_differs_latches_native_refuses() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let facts = || vec![fact("win-a", "out-a", 0)];
    let mut boot = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        facts(),
        vec![],
        "",
        None,
        None,
    );
    boot.inventory_complete = false;
    accepted(&mut engine, &boot);
    let null_data = encode_intent_data(&IntentData::new(None, None));
    let mut second = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        facts(),
        vec![],
        "",
        None,
        None,
    );
    second.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: null_data,
    }];
    second.inventory_complete = false;
    accepted(&mut engine, &second);
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(None, None)),
        "null/null cached first"
    );
    assert!(!engine.sticky_saved_incomplete);
    let valid_data = encode_intent_data(&IntentData::new(Some(false), Some(false)));
    let mut third = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        facts(),
        vec![],
        "",
        None,
        None,
    );
    third.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: valid_data,
    }];
    third.inventory_complete = false;
    accepted(&mut engine, &third);
    assert!(
        engine.sticky_saved_incomplete,
        "differing valid after null/null latches"
    );
    assert!(
        !engine
            .sticky_saved
            .contains_key(&WindowId("win-a".to_owned())),
        "bogus null/null prior dropped, newcomer never recached"
    );
    assert_eq!(
        engine.pending_intent.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(false), Some(false))),
        "composed admission keeps old behavior: valid pending stands"
    );
    // Retained pending keeps sticky_current true, so same-state ON would
    // NoEffect-accept before the fence; OFF reaches the exact-knowledge
    // gate while raw stays known false (never a StateUnknown mask).
    match engine.handle_report_legacy(&cmd(4, 1, facts(), set_sticky("win-a", false))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected fail-closed native refusal"
        ),
        other => panic!("expected fail-closed native refusal, got {other:?}"),
    }
    assert!(engine.sticky_command_save.is_none(), "no save staged");
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "no native target minted"
    );
}

#[test]
fn valid_first_then_null_null_differs_latches_native_refuses() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let facts = || vec![fact("win-a", "out-a", 0)];
    let mut boot = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        facts(),
        vec![],
        "",
        None,
        None,
    );
    boot.inventory_complete = false;
    accepted(&mut engine, &boot);
    let valid_data = encode_intent_data(&IntentData::new(Some(false), Some(false)));
    let mut second = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        facts(),
        vec![],
        "",
        None,
        None,
    );
    second.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: valid_data,
    }];
    second.inventory_complete = false;
    accepted(&mut engine, &second);
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(false), Some(false))),
        "valid shape cached first"
    );
    assert!(!engine.sticky_saved_incomplete);
    let null_data = encode_intent_data(&IntentData::new(None, None));
    let mut third = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        facts(),
        vec![],
        "",
        None,
        None,
    );
    third.intent_records = vec![IntentRecord {
        win: WindowId("win-a".to_owned()),
        data: null_data,
    }];
    third.inventory_complete = false;
    accepted(&mut engine, &third);
    assert!(
        engine.sticky_saved_incomplete,
        "differing null/null after valid latches"
    );
    assert!(
        !engine
            .sticky_saved
            .contains_key(&WindowId("win-a".to_owned())),
        "bogus valid prior dropped, null/null never overwrites"
    );
    assert_eq!(
        engine.pending_intent.get(&WindowId("win-a".to_owned())),
        Some(&IntentData::new(Some(false), Some(false))),
        "composed admission keeps old behavior: valid pending retained"
    );
    // Retained pending keeps sticky_current true, so same-state ON would
    // NoEffect-accept before the fence; OFF reaches the exact-knowledge
    // gate while raw stays known false (never a StateUnknown mask).
    match engine.handle_report_legacy(&cmd(4, 1, facts(), set_sticky("win-a", false))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected fail-closed native refusal"
        ),
        other => panic!("expected fail-closed native refusal, got {other:?}"),
    }
    assert!(engine.sticky_command_save.is_none(), "no save staged");
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "no native target minted"
    );
}
