//! Focused real-engine tests for the explicit ToggleFloat SAVE publication,
//! sticky preservation, and the pre-mutation bound.
//!
//! Restore-side behavior (first-admission import, conflicts, late rows)
//! stays in `world_intent_restore_tests`; this module covers the outbound
//! lane: canonical true/false saves, no-save boundaries (no-space,
//! duplicate/consumed, refused/held/hints-null, automatic, sticky blocks),
//! save round-trips through rehandshake, sticky block lifecycle, and the
//! sizing bound (`max_intent_save_bound`, one-save reply fit, oversized
//! nonmutation with a save command).

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::intent_data::{IntentData, encode_intent_data};
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{IntentRecord, WorldCommand, WorldReportV4};
use crate::world_v4_support_tests::{accepted, fact, report_full, screen};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn zero_gap_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the save
            // lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "outer".to_owned(),
            crate::connection::RawSettingValue::Number(0.0),
        )],
    )
    .expect("generation")
}

fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(40),
        min_h: Some(90),
        max_w: Some(40),
        max_h: Some(90),
    }
}

fn fixed_fact(window: &str, output: &str, x: i32) -> crate::world_report::WindowFact {
    let mut f = fact(window, output, x);
    f.hints = Some(fixed_hints());
    f
}

fn with_records(mut report: WorldReportV4, records: Vec<IntentRecord>) -> WorldReportV4 {
    report.intent_records = records;
    report
}

fn record(win: &str, data: &str) -> IntentRecord {
    IntentRecord {
        win: WindowId(win.to_owned()),
        data: data.to_owned(),
    }
}

fn float_true() -> String {
    encode_intent_data(&IntentData::new(Some(true), None))
}

fn tile_false() -> String {
    encode_intent_data(&IntentData::new(Some(false), None))
}

fn sticky_float() -> String {
    encode_intent_data(&IntentData::new(Some(true), Some(false)))
}

fn sticky_tile() -> String {
    encode_intent_data(&IntentData::new(Some(false), Some(true)))
}

fn toggle(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    })
}

fn toggle_report(seq: u64, windows: Vec<crate::world_report::WindowFact>) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        Some(seq),
        Some(toggle("win-a")),
    )
}

fn assert_save(reply: &crate::world_report::WorldReplyV4, win: &str, data: &str) {
    assert_eq!(reply.intent_writes.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_writes[0].win, WindowId(win.to_owned()));
    assert_eq!(reply.intent_writes[0].data.as_deref(), Some(data));
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

fn is_floated(engine: &WorldEngine, win: &str) -> bool {
    let id = WindowId(win.to_owned());
    engine.engine.domain_keys().iter().any(|key| {
        engine
            .engine
            .session(key)
            .is_some_and(|s| s.is_exception(&id))
    })
}

fn row_for<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    win: &str,
) -> Option<&'a crate::world_report::WindowDesire> {
    reply.windows.iter().find(|row| row.lifetime.0 == win)
}

fn rehandshake(engine: &mut WorldEngine) -> String {
    // Default capabilities: host-owned mode would change homing under the
    // new generation, so round-trips stay on the plain lane. Native sticky
    // and maximize stay supported: the fixtures carry native sticky and
    // maximize facts.
    let reply = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "handshake-2".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: Some("example.app".to_owned()),
            executable: Some("example".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    });
    assert!(reply.kind.is_none());
    let generation = reply.generation.expect("generation");
    // A fresh handshake resets settings acceptance: re-accept before any
    // staged report or every report bootstraps instead of restoring.
    let settings = engine.handle_settings(&crate::connection::SettingsRequest {
        adapter: "adapter-2".to_owned(),
        generation: generation.clone(),
        request: "post-handshake-settings".to_owned(),
        revision: 0,
        values: vec![(
            "mode".to_owned(),
            crate::connection::RawSettingValue::String(
                crate::workspace_scope::WORKSPACE_MODE_TOKENS[0].to_owned(),
            ),
        )],
    });
    assert!(settings.is_some(), "settings re-accepted");
    generation
}

fn tiny_screen(output: &str, x: i32) -> crate::world_report::ScreenReport {
    crate::world_report::ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(Rect {
            x,
            y: 0,
            w: 10,
            h: 10,
        }),
        work_area: Some(Rect {
            x,
            y: 0,
            w: 10,
            h: 10,
        }),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

#[test]
fn toggle_tile_to_float_emits_true_save() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(is_tiled(&engine, "win-a"));
    let reply = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&reply, "win-a", &float_true());
    assert!(is_floated(&engine, "win-a"));
    match &reply.command_result {
        Some(crate::world_report::CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, "win-a");
            assert!(rect.is_some(), "float carries a rect");
            assert!(!no_space);
        }
        other => panic!("expected float result, got {other:?}"),
    }
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    let row = row_for(&reply, "win-a").expect("float row");
    assert!(row.floating, "{row:?}");
}

#[test]
fn toggle_float_to_tile_emits_false_save_for_ordinary_nonfixed() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let floated = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&floated, "win-a", &float_true());
    // Ordinary non-fixed facts echo the float; the second toggle unfloats.
    let reply = accepted(
        &mut engine,
        &toggle_report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&reply, "win-a", &tile_false());
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn automatic_float_unfloat_records_tile_save() {
    // Invariant: an explicit user unfloat of an automatic float persists
    // the commanded tile as a canonical false save. No unfloat guard: this
    // records user tile intent (approved fixed-window restart persistence),
    // never an intentional float.
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fixed_fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    let reply = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fixed_fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&reply, "win-a", &tile_false());
    assert!(is_tiled(&engine, "win-a"));
}

#[test]
fn no_space_retry_emits_no_save_and_stays_float() {
    let mut engine = zero_gap_engine();
    let screens = vec![tiny_screen("out-a", 0)];
    let seed: Vec<crate::world_report::WindowFact> = (0..5)
        .map(|i| fact(&format!("w{i}"), "out-a", i * 10))
        .collect();
    let first = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            screens.clone(),
            seed.clone(),
            vec![],
            "w0",
            None,
            None,
        ),
    );
    let victim = first
        .windows
        .iter()
        .find(|row| row.floating)
        .map(|row| row.lifetime.0.clone())
        .expect("exhaustion floats");
    let echo: Vec<crate::world_report::WindowFact> = seed
        .iter()
        .map(|w| {
            let mut w = w.clone();
            if w.window.0 == victim {
                w.transient = Some(true);
            }
            w
        })
        .collect();
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            screens,
            echo,
            vec![],
            "w1",
            Some(2),
            Some(toggle(&victim)),
        ),
    );
    match &reply.command_result {
        Some(crate::world_report::CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, victim);
            assert_eq!(*rect, None);
            assert!(*no_space, "still exhausted");
        }
        other => panic!("expected float result, got {other:?}"),
    }
    assert!(
        reply.intent_writes.is_empty(),
        "no-space retry manufactures no tile: {:?}",
        reply.intent_writes
    );
    let row = row_for(&reply, &victim).expect("victim row");
    assert!(row.floating, "{row:?}");
}

#[test]
fn duplicate_repeats_cached_save() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let report = toggle_report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    let first = accepted(&mut engine, &report);
    assert_save(&first, "win-a", &float_true());
    match engine.handle_report(&report) {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.intent_writes, first.intent_writes);
            assert_save(&cached, "win-a", &float_true());
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
}

#[test]
fn consumed_command_report_emits_no_save() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let first = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&first, "win-a", &float_true());
    // Same command sequence on a fresh report reconciles only.
    let mut consumed = toggle_report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    consumed.command_seq = Some(2);
    match engine.handle_report(&consumed) {
        WorldOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert_eq!(reply.based_on_observation_seq, 3);
            assert!(reply.command_result.is_none(), "{:?}", reply.command_result);
            assert!(
                reply.intent_writes.is_empty(),
                "consumed command emits nothing: {:?}",
                reply.intent_writes
            );
        }
        other => panic!("expected consumed, got {other:?}"),
    }
    assert!(is_floated(&engine, "win-a"));
}

#[test]
fn refused_unknown_window_preserves_state_for_retry() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let revision = engine.state_revision;
    let bad = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        vec![],
        "",
        Some(2),
        Some(toggle("win-nope")),
    );
    match engine.handle_report(&bad) {
        WorldOutcome::Accepted(reply) => {
            // Unknown toggle subject never reaches the engine: the staged
            // observation cannot prove it.
            assert_eq!(
                reply.command_result,
                Some(crate::world_report::CommandResult::Rejected {
                    kind: crate::world_report::CommandRejectKind::OperationRefused
                }),
                "expected refusal"
            );
        }
        other => panic!("expected refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert_eq!(engine.command_highwater, Some(2));
    // The consumed sequence stays consumed; a fresh user press works once
    // the window exists.
    let retry = toggle_report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    let reply = accepted(&mut engine, &retry);
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn held_scope_toggle_refuses_without_mutation() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let revision = engine.state_revision;
    let mut held_screen = screen("out-a", 0);
    held_screen.complete = false;
    let held = report_full(
        2,
        2,
        vec![held_screen],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        vec![],
        "",
        Some(2),
        Some(toggle("win-a")),
    );
    match engine.handle_report(&held) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(crate::world_report::CommandResult::Rejected {
                kind: crate::world_report::CommandRejectKind::OperationRefused
            }),
            "expected held refusal"
        ),
        other => panic!("expected held refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "held refusal issues");
    assert_eq!(engine.command_highwater, Some(2));
    let retry = toggle_report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    let reply = accepted(&mut engine, &retry);
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn hints_null_toggle_refuses_without_mutation() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let revision = engine.state_revision;
    let mut unreadable = fact("win-a", "out-a", 0);
    unreadable.hints = None;
    let refused = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![unreadable, fact("win-b", "out-a", 400)],
        vec![],
        "",
        Some(2),
        Some(toggle("win-a")),
    );
    match engine.handle_report(&refused) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(crate::world_report::CommandResult::Rejected {
                kind: crate::world_report::CommandRejectKind::OperationRefused
            }),
            "expected refusal"
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert_eq!(engine.command_highwater, Some(2));
    let retry = toggle_report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    let reply = accepted(&mut engine, &retry);
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn rehandshake_float_save_restores_without_resave_or_placement() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let toggled = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&toggled, "win-a", &float_true());
    let generation = rehandshake(&mut engine);
    let mut restore = with_records(
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 100), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-a", &float_true())],
    );
    restore.adapter_incarnation = "adapter-2".to_owned();
    restore.core_generation = generation;
    let reply = accepted(&mut engine, &restore);
    assert!(
        reply.intent_writes.is_empty(),
        "restore consumes the save without re-saving: {:?}",
        reply.intent_writes
    );
    assert!(is_floated(&engine, "win-a"));
    let row = row_for(&reply, "win-a").expect("float row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
}

#[test]
fn rehandshake_tile_save_restores_without_resave() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let untoggled = accepted(
        &mut engine,
        &toggle_report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&untoggled, "win-a", &tile_false());
    let generation = rehandshake(&mut engine);
    let mut restore = with_records(
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-a", &tile_false())],
    );
    restore.adapter_incarnation = "adapter-2".to_owned();
    restore.core_generation = generation;
    let reply = accepted(&mut engine, &restore);
    assert!(
        reply.intent_writes.is_empty(),
        "restore consumes the save without re-saving: {:?}",
        reply.intent_writes
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn sticky_restore_blocks_save_and_clears_later_toggle_with_hold() {
    // Sticky restore commits a semantic float: the deferred record blocks
    // ordinary saves, while a ToggleFloat against the restored subject
    // clears sticky through the native hold path (tile-base save, runtime
    // stays floated until correlated clear proof).
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    assert!(
        reply.intent_writes.is_empty(),
        "restore publishes no save: {:?}",
        reply.intent_writes
    );
    assert!(is_floated(&engine, "win-a"), "sticky restores as float");
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    assert!(engine.pending_intent.is_empty());
    // ToggleFloat against the restored subject clears sticky through the
    // native hold path: tile-base save, runtime stays floated, provenance
    // and block intact until settlement.
    let sticky_prior = engine.restored_sticky.clone();
    let cleared = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&cleared, "win-a", &tile_false());
    assert_eq!(engine.restored_sticky, sticky_prior, "provenance intact");
    assert!(is_floated(&engine, "win-a"), "still floated during hold");
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "clear preserves the save block"
    );
    // A safe retry for the ordinary non-sticky window still wins.
    let retry = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(3),
            Some(toggle("win-b")),
        ),
    );
    assert_save(&retry, "win-b", &float_true());
}

#[test]
fn sticky_toggle_clearhold_preserves_float_with_nonsticky_retry() {
    // Native clearhold: ToggleFloat against the sticky-restored exception
    // queues the tile-base save and holds the float runtime (never retiles
    // before correlated clear proof), while an ordinary window still
    // toggles and saves.
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    let sticky_prior = engine.restored_sticky.clone();
    let cleared = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&cleared, "win-a", &tile_false());
    assert_eq!(engine.restored_sticky, sticky_prior, "provenance intact");
    assert!(is_floated(&engine, "win-a"), "exception survives clearhold");
    assert!(!is_tiled(&engine, "win-a"), "never retiled by clearhold");
    let retry = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(3),
            Some(toggle("win-b")),
        ),
    );
    assert_save(&retry, "win-b", &float_true());
    assert!(is_floated(&engine, "win-a"), "sticky float undisturbed");
}

#[test]
fn earlier_report_sticky_row_blocks_later_toggle_save() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_tile())],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    // Sticky dominates the float lane: even a float:false record restores
    // as a semantic float with the exact prior, never a tile.
    assert!(is_floated(&engine, "win-a"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true)
    );
    // Record-free report preserves the block through admission.
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    // ToggleFloat against the restored sticky subject clears through the
    // native hold path: tile-base save, runtime stays floated, provenance
    // and block intact until settlement.
    let sticky_prior = engine.restored_sticky.clone();
    let cleared = accepted(
        &mut engine,
        &toggle_report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&cleared, "win-a", &tile_false());
    assert_eq!(engine.restored_sticky, sticky_prior, "provenance intact");
    assert!(is_floated(&engine, "win-a"), "still floated during hold");
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "clear preserves the block"
    );
}

#[test]
fn mixed_duplicate_conflicting_sticky_rows_block_and_conflict() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![
                record("win-a", &sticky_float()),
                record("win-a", &tile_false()),
            ],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "sticky scan runs before conflict handling"
    );
    assert!(
        !engine
            .intent_conflicts
            .contains(&WindowId("win-a".to_owned())),
        "successful admission clears conflicts, unlike blocks"
    );
    assert!(engine.pending_intent.is_empty(), "no partial restore");
    let reply = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert!(
        reply.intent_writes.is_empty(),
        "conflicting sticky suppresses: {:?}",
        reply.intent_writes
    );
    assert!(is_floated(&engine, "win-a"));
}

#[test]
fn later_nonsticky_row_does_not_unblock() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    accepted(
        &mut engine,
        &with_records(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &tile_false())],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "later non-sticky never clears the block"
    );
    assert!(is_floated(&engine, "win-a"), "sticky restore floats");
    // ToggleFloat against the restored subject clears through the native
    // hold path: tile-base save, runtime stays floated.
    let sticky_prior = engine.restored_sticky.clone();
    let cleared = accepted(
        &mut engine,
        &toggle_report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&cleared, "win-a", &tile_false());
    assert_eq!(engine.restored_sticky, sticky_prior, "provenance intact");
    assert!(is_floated(&engine, "win-a"), "still floated during hold");
}

#[test]
fn late_sticky_after_admission_does_not_block_runtime_save() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-a".to_owned()))
    );
    accepted(
        &mut engine,
        &with_records(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "late sticky after admission is ignored"
    );
    let reply = accepted(
        &mut engine,
        &toggle_report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn sticky_block_survives_minimize_and_record_free_reports() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_tile())],
        ),
    );
    let mut minimized = fact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![minimized, fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "minimize keeps the block"
    );
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    // ToggleFloat against the restored sticky subject clears through the
    // native hold path: tile-base save, runtime stays floated, block and
    // provenance intact.
    let sticky_prior = engine.restored_sticky.clone();
    let cleared = accepted(
        &mut engine,
        &toggle_report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&cleared, "win-a", &tile_false());
    assert_eq!(engine.restored_sticky, sticky_prior, "provenance intact");
    assert!(is_floated(&engine, "win-a"), "still floated during hold");
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn sticky_block_survives_exclusion_gap() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    let mut excluded = fact("win-a", "out-a", 0);
    excluded.owned = Some(true);
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400), excluded],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "exclusion keeps the block"
    );
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400), fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    // ToggleFloat against the restored sticky subject clears through the
    // native hold path: tile-base save, runtime stays floated.
    let sticky_prior = engine.restored_sticky.clone();
    let cleared = accepted(
        &mut engine,
        &toggle_report(
            4,
            vec![fact("win-b", "out-a", 400), fact("win-a", "out-a", 0)],
        ),
    );
    assert_save(&cleared, "win-a", &tile_false());
    assert_eq!(engine.restored_sticky, sticky_prior, "provenance intact");
    assert!(is_floated(&engine, "win-a"), "still floated during hold");
}

#[test]
fn close_prunes_sticky_block_and_new_lifetime_saves() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_tile())],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    let mut closed = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-b", "out-a", 400)],
        vec![],
        "",
        None,
        None,
    );
    closed.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut engine, &closed);
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "explicit close ends the lifetime block"
    );
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &toggle_report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn own_process_fact_prunes_sticky_block() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    let mut own = fact("win-a", "out-a", 0);
    own.process_id = Some("test-own-pid".to_owned());
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![own, fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "proven own-process drops the block"
    );
}

#[test]
fn complete_absence_prunes_footprint_free_sticky_block() {
    // A sticky subject that never gains engine footprint (no fact) is
    // pruned by complete inventory: an incomplete gap retains it first,
    // mirroring the pending orphan lifecycle. Admitted windows keep their
    // session slot across a gap (never inferred closed), so their blocks
    // stay fail-closed until explicit close.
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let mut gapped = with_records(
        report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-a", &sticky_tile())],
    );
    gapped.inventory_complete = false;
    accepted(&mut engine, &gapped);
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "incomplete gap retains the orphan block"
    );
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned())),
        "proven complete departure drops the footprint-free block"
    );
}

#[test]
fn bootstrap_ingest_prunes_absent_sticky_on_complete() {
    let mut engine = engine();
    let mut pre = with_records(
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-x", &sticky_float())],
    );
    pre.inventory_complete = false;
    let boot = accepted(&mut engine, &pre);
    assert!(!boot.ready, "incomplete first report bootstraps");
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-x".to_owned())),
        "bootstrap ingests sticky blocks"
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-x".to_owned())),
        "complete inventory without the subject prunes"
    );
}

#[test]
fn joint_bound_overflow_latches_saturated_fail_closed() {
    let mut engine = engine();
    for i in 0..crate::world_report::MAX_INTENT_RECORDS {
        engine.pending_intent.insert(
            WindowId(format!("win-{i:05}")),
            crate::intent_data::IntentData::new(Some(false), None),
        );
    }
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-new", &sticky_float())],
        ),
    );
    assert!(engine.intent_sticky_saturated, "overflow latches");
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-new".to_owned())),
        "no room means no block, never a dropped save gate"
    );
    let reply = accepted(
        &mut engine,
        &toggle_report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert!(
        reply.intent_writes.is_empty(),
        "saturation suppresses every save: {:?}",
        reply.intent_writes
    );
    assert!(is_floated(&engine, "win-a"), "command still wins");
    let _ = rehandshake(&mut engine);
    assert!(
        !engine.intent_sticky_saturated,
        "handshake resets the latch"
    );
    assert!(engine.intent_sticky_blocks.is_empty());
}

#[test]
fn joint_bound_holds_one_block_then_latches() {
    let mut engine = engine();
    for i in 0..crate::world_report::MAX_INTENT_RECORDS - 1 {
        engine.pending_intent.insert(
            WindowId(format!("win-{i:05}")),
            crate::intent_data::IntentData::new(Some(false), None),
        );
    }
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &sticky_float())],
        ),
    );
    assert!(!engine.intent_sticky_saturated);
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    // The first report's finish prunes the filler (not live, not known),
    // so refill to the joint cap before the second sticky row.
    for i in 0..crate::world_report::MAX_INTENT_RECORDS - 1 {
        engine.pending_intent.insert(
            WindowId(format!("win-refill-{i:05}")),
            crate::intent_data::IntentData::new(Some(true), None),
        );
    }
    assert_eq!(
        engine.intent_ledger_used(),
        crate::world_report::MAX_INTENT_RECORDS
    );
    accepted(
        &mut engine,
        &with_records(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-b", &sticky_tile())],
        ),
    );
    assert!(engine.intent_sticky_saturated, "full ledger latches");
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-b".to_owned()))
    );
}

#[test]
fn joint_cap_sticky_occupancy_fails_conflict_allocs_closed() {
    // Discriminator for the joint ledger: with sticky blocks occupying
    // slots, pending/conflict branches that counted only
    // pending+conflicts would wrongly admit. Drives real ingest with the
    // ledger exactly full: a raw duplicate (win-d), a cross-report
    // differing value (win-p), and a fresh sticky (win-t).
    let mut engine = engine();
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-s", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-s", &sticky_float())],
        ),
    );
    let mut partial = with_records(
        report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-s", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-p", &tile_false())],
    );
    partial.inventory_complete = false;
    accepted(&mut engine, &partial);
    assert!(
        engine
            .pending_intent
            .get(&WindowId("win-p".to_owned()))
            .is_some_and(|data| data.float == Some(false))
    );
    // Fill to the joint cap: one sticky block plus one sticky-prior
    // provenance plus one pending plus filler. Restored sticky counts, so
    // the filler leaves room for exactly the joint total.
    for i in 0..crate::world_report::MAX_INTENT_RECORDS - 3 {
        engine.pending_intent.insert(
            WindowId(format!("win-fill-{i:06}")),
            crate::intent_data::IntentData::new(Some(false), None),
        );
    }
    assert_eq!(
        engine.intent_ledger_used(),
        crate::world_report::MAX_INTENT_RECORDS
    );
    let mut minimized = fact("win-d", "out-a", 200);
    minimized.minimized = Some(true);
    let mut capped = with_records(
        report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![
                fact("win-s", "out-a", 0),
                fact("win-b", "out-a", 400),
                minimized,
            ],
            vec![],
            "",
            None,
            None,
        ),
        vec![
            record("win-d", &tile_false()),
            record("win-d", &float_true()),
            record("win-p", &float_true()),
            record("win-t", &sticky_tile()),
        ],
    );
    capped.inventory_complete = false;
    let reply = accepted(&mut engine, &capped);
    assert!(reply.intent_writes.is_empty());
    // win-d: raw duplicate refused at a full ledger (pending+conflicts
    // alone had room: MAX-1). win-p: differing value freed its pending
    // slot, so its conflict fits. win-t: sticky finds no room and latches.
    assert!(
        !engine
            .intent_conflicts
            .contains(&WindowId("win-d".to_owned())),
        "duplicate conflict refused fail-closed"
    );
    assert!(
        engine
            .intent_conflicts
            .contains(&WindowId("win-p".to_owned())),
        "freed slot admits the differing-value conflict"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-d".to_owned()))
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-p".to_owned()))
    );
    assert!(engine.intent_sticky_saturated, "sticky overflow latches");
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-t".to_owned()))
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-s".to_owned())),
        "existing block survives"
    );
    assert_eq!(
        engine.intent_ledger_used(),
        crate::world_report::MAX_INTENT_RECORDS
    );
    // No arbitrary restore authority from the refused rows.
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-d".to_owned()))
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-p".to_owned()))
    );
    assert!(!is_floated(&engine, "win-d"));
    assert!(!is_tiled(&engine, "win-p") && !is_floated(&engine, "win-p"));
}

#[test]
fn joint_cap_room_inserts_duplicate_and_cross_report_conflicts() {
    // Positive control for the capped test: with room, a raw duplicate
    // conflict and a cross-report differing-value conflict both store,
    // stay bounded, and confer no restore authority.
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let mut minimized = fact("win-d", "out-a", 200);
    minimized.minimized = Some(true);
    let mut dup = with_records(
        report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400), minimized],
            vec![],
            "",
            None,
            None,
        ),
        vec![
            record("win-d", &tile_false()),
            record("win-d", &float_true()),
        ],
    );
    dup.inventory_complete = false;
    accepted(&mut engine, &dup);
    assert!(
        engine
            .intent_conflicts
            .contains(&WindowId("win-d".to_owned())),
        "raw duplicate stores with room"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-d".to_owned()))
    );
    let mut first = with_records(
        report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400), fact("win-d", "out-a", 200)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-p", &tile_false())],
    );
    first.inventory_complete = false;
    accepted(&mut engine, &first);
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-p".to_owned()))
    );
    // NOTE: win-d now carries an ordinary (non-minimized) fact, so it
    // admits tiled here and its conflict clears (blocks survive admission,
    // unlike conflicts). The win-p assertions below are unaffected.
    let mut second = with_records(
        report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400), fact("win-d", "out-a", 200)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-p", &float_true())],
    );
    second.inventory_complete = false;
    accepted(&mut engine, &second);
    assert!(
        engine
            .intent_conflicts
            .contains(&WindowId("win-p".to_owned())),
        "differing cross-report value stores with room"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-p".to_owned()))
    );
    assert!(engine.intent_ledger_used() <= crate::world_report::MAX_INTENT_RECORDS);
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-d".to_owned()))
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-p".to_owned()))
    );
}

#[test]
fn save_data_canonical_widths_match_bound_constant() {
    assert_eq!(float_true().len(), 39);
    assert_eq!(tile_false().len(), 40);
    assert_eq!(crate::world_sizing::json_string_len(&float_true()), 47);
    assert_eq!(crate::world_sizing::json_string_len(&tile_false()), 48);
    assert_eq!(crate::world_sizing::MAX_INTENT_SAVE_DATA_JSON, 48);
}

#[test]
fn max_bound_reserves_one_row_only_for_toggle_float() {
    use crate::world_sizing::json_string_len;
    use crate::world_sizing_v4::max_intent_save_bound_v4 as max_intent_save_bound;
    let toggle = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        Some(1),
        Some(toggle("win-a")),
    );
    assert_eq!(
        max_intent_save_bound(&toggle),
        16 + json_string_len("win-a") + 48
    );
    // A window id needing escapes still bounds exactly.
    let mut quoted = toggle.clone();
    quoted.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: "w\"in\\q".to_owned(),
        float_rect: None,
    }));
    assert_eq!(
        max_intent_save_bound(&quoted),
        16 + json_string_len("w\"in\\q") + 48
    );
    // A valid max-length opaque id reserves the documented maximum row:
    // 16 punctuation plus the 130-byte quoted id plus the 48-byte value.
    let mut maxed = toggle.clone();
    maxed.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: "w".repeat(128),
        float_rect: None,
    }));
    assert_eq!(max_intent_save_bound(&maxed), 194);
    let plain = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    assert_eq!(max_intent_save_bound(&plain), 0);
    let focus = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        Some(1),
        Some(WorldCommand::Focus {
            window: WindowId("win-a".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    assert_eq!(max_intent_save_bound(&focus), 0);
}

#[test]
fn max_bound_reserves_one_save_row_for_sticky_commands() {
    use crate::world_sizing::json_string_len;
    use crate::world_sizing_v4::max_intent_save_bound_v4 as max_intent_save_bound;
    let sticky = |command: WorldCommand| {
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![],
            vec![],
            "",
            Some(1),
            Some(command),
        )
    };
    let set = sticky(WorldCommand::SetSticky {
        window: WindowId("win-a".to_owned()),
        value: true,
    });
    assert_eq!(
        max_intent_save_bound(&set),
        16 + json_string_len("win-a") + 48
    );
    let toggle = sticky(WorldCommand::ToggleSticky {
        window: WindowId("win-a".to_owned()),
    });
    assert_eq!(
        max_intent_save_bound(&toggle),
        16 + json_string_len("win-a") + 48
    );
    // A window id needing escapes still bounds exactly.
    let mut quoted = set.clone();
    quoted.command = Some(WorldCommand::ToggleSticky {
        window: WindowId("w\"in\\q".to_owned()),
    });
    assert_eq!(
        max_intent_save_bound(&quoted),
        16 + json_string_len("w\"in\\q") + 48
    );
    // A valid max-length opaque id reserves the documented maximum row:
    // 16 punctuation plus the 130-byte quoted id plus the 48-byte value.
    let mut maxed = set.clone();
    maxed.command = Some(WorldCommand::SetSticky {
        window: WindowId("w".repeat(128)),
        value: false,
    });
    assert_eq!(max_intent_save_bound(&maxed), 194);
    // Non-save commands reserve nothing.
    let plain = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    assert_eq!(max_intent_save_bound(&plain), 0);
    let view = sticky(WorldCommand::SelectTrailing {
        output: Some(OutputId("out-a".to_owned())),
    });
    assert_eq!(max_intent_save_bound(&view), 0);
    let focus = sticky(WorldCommand::Focus {
        window: WindowId("win-a".to_owned()),
        direction: "left".to_owned(),
    });
    assert_eq!(max_intent_save_bound(&focus), 0);
}

#[test]
fn sticky_save_canonical_widths_match_bound_constant() {
    use crate::world_sizing::json_string_len;
    // Staged sticky family: sticky-on `(Some(true), prior)`, sticky-off
    // `(prior, None)`, and the sticky-clearing toggle `(Some(false),
    // None)`. The widest escaped forms need the full 48-byte reserve.
    let on_tiled = encode_intent_data(&IntentData::new(Some(true), Some(false)));
    let on_float = encode_intent_data(&IntentData::new(Some(true), Some(true)));
    let off = encode_intent_data(&IntentData::new(Some(false), None));
    assert_eq!(on_float.len(), 39);
    assert_eq!(on_tiled.len(), 40);
    assert_eq!(off.len(), 40);
    assert_eq!(json_string_len(&on_float), 47);
    assert_eq!(json_string_len(&on_tiled), 48);
    assert_eq!(json_string_len(&off), 48);
    assert_eq!(crate::world_sizing::MAX_INTENT_SAVE_DATA_JSON, 48);
}

#[test]
fn reply_upper_bound_adds_exactly_one_save_row() {
    use crate::world_sizing_v4::{
        max_intent_save_bound_v4 as max_intent_save_bound,
        reply_upper_bound_v4 as reply_upper_bound, report_estimate_v4 as report_estimate,
    };
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    let toggle = toggle_report(2, windows.clone());
    let plain = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        None,
        None,
    );
    let bound = max_intent_save_bound(&toggle);
    assert!(bound > 0);
    assert_eq!(
        reply_upper_bound(&toggle, 0, 1, 1, 0, 0) - reply_upper_bound(&plain, 0, 1, 1, 0, 0),
        (report_estimate(&toggle) - report_estimate(&plain)) + bound
    );
}

#[test]
fn actual_one_save_reply_fits_estimate_with_exact_row() {
    use crate::world_sizing::{intent_writes_estimate, json_string_len};
    use crate::world_sizing_v4::{
        reply_estimate_v4 as reply_estimate, reply_exact_bytes_v4,
        reply_upper_bound_v4 as reply_upper_bound,
    };
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let input = toggle_report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    // Pre-mutation upper bound for this exact input, read the way the
    // commit gate reads it (retained/logical counts before any mutation).
    let pre_bound = reply_upper_bound(
        &input,
        engine.retained.len() + engine.retained_floats.len(),
        engine.engine.domain_keys().len(),
        input.screens.len(),
        engine.staged.facts.len() + engine.staged.held_windows.len(),
        engine.retained_publish_footprint(),
    );
    let reply = accepted(&mut engine, &input);
    assert_save(&reply, "win-a", &float_true());
    let data = reply.intent_writes[0].data.clone().expect("save data");
    assert_eq!(
        intent_writes_estimate(&reply.intent_writes),
        18 + 16 + json_string_len("win-a") + json_string_len(&data)
    );
    // Exact v4 reply bytes (length arithmetic mirroring the protocol
    // `encode_reply_v4` writer field-by-field; no core v4 writer exists).
    // The save payload itself is already pinned typed by `assert_save`
    // above; the wire-substring check has no typed-v4-writer counterpart.
    let actual = reply_exact_bytes_v4(&reply);
    assert!(
        reply_estimate(&reply) >= actual,
        "estimate {} covers actual {}",
        reply_estimate(&reply),
        actual
    );
    assert!(
        actual <= pre_bound,
        "exact one-save reply {} fits the pre-mutation upper bound {pre_bound}",
        actual
    );
}

#[test]
fn oversized_toggle_report_rejected_without_mutation() {
    use crate::world_report::WORLD_MAX_REQUEST_BYTES;
    use crate::world_sizing_v4::report_estimate_v4 as report_estimate;
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let revision = engine.state_revision;
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..1500 {
        let mut padded = fact(&format!("win-pad-{i:04}"), "out-a", 0);
        padded.title = Some("t".repeat(3000));
        windows.push(padded);
    }
    let oversized = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        Some(2),
        Some(toggle("win-a")),
    );
    assert!(
        report_estimate(&oversized) > WORLD_MAX_REQUEST_BYTES,
        "fixture really is oversized"
    );
    match engine.handle_report(&oversized) {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_OVERSIZED);
        }
        other => panic!("expected oversized, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision, "oversized mutates nothing");
    assert!(engine.command_highwater.is_none(), "command unconsumed");
    // The same save command still works once the report fits.
    let retry = toggle_report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    let reply = accepted(&mut engine, &retry);
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn reply_bound_toggle_report_rejected_without_mutation() {
    // Pre-mutation REPLY bound refusal (not the request-size gate): the
    // report itself fits, but the worst-case reply does not. The padding
    // is tuned so the bound WITHOUT the save row still fits while the
    // bound WITH the ToggleFloat save row exceeds it, proving the new
    // reservation is the discriminator.
    use crate::world_report::WORLD_MAX_REPLY_BYTES;
    use crate::world_sizing_v4::{
        max_intent_save_bound_v4 as max_intent_save_bound,
        reply_upper_bound_v4 as reply_upper_bound, report_estimate_v4 as report_estimate,
    };
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let save_bound = max_intent_save_bound(&toggle_report(2, Vec::new()));
    assert_eq!(save_bound, 71);
    let logical = engine.engine.domain_keys().len();
    let bound_of = |windows: &[crate::world_report::WindowFact]| {
        let report = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.to_vec(),
            vec![],
            "",
            Some(2),
            Some(toggle("win-a")),
        );
        reply_upper_bound(
            &report,
            0,
            logical,
            1,
            0,
            engine.retained_publish_footprint(),
        )
    };
    // Coarse: drop small pad windows until the saveless bound fits.
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) - save_bound > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    // Fine: grow one bounded title on an existing pad window - each
    // ASCII title byte adds exactly one to estimate and bound alike (plus
    // 2 for the null-to-string swap: absent titles count `null` as 4 while
    // a string counts its quotes) - landing the saveless bound within one
    // save row below the cap.
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(70)
        .saturating_sub(bound_of(&windows) - save_bound)
        + 2;
    windows[2].title = Some("t".repeat(deficit));
    let report = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        Some(2),
        Some(toggle("win-a")),
    );
    let without_save = reply_upper_bound(
        &report,
        0,
        logical,
        1,
        0,
        engine.retained_publish_footprint(),
    ) - save_bound;
    assert!(
        report_estimate(&report) <= crate::world_report::WORLD_MAX_REQUEST_BYTES,
        "request itself fits; only the reply bound trips"
    );
    assert!(
        without_save <= WORLD_MAX_REPLY_BYTES,
        "saveless bound fits: {without_save}"
    );
    assert!(
        without_save + save_bound > WORLD_MAX_REPLY_BYTES,
        "save row is the discriminator: {without_save} + {save_bound}"
    );
    let revision = engine.state_revision;
    let admitted = engine.intent_admitted.clone();
    let pending = engine.pending_intent.clone();
    let conflicts = engine.intent_conflicts.clone();
    let blocks = engine.intent_sticky_blocks.clone();
    let restored = engine.restored_floats.clone();
    match engine.handle_report(&report) {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REPLY_SIZE);
        }
        other => panic!("expected reply-oversize, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision, "refusal mutates nothing");
    assert!(engine.command_highwater.is_none(), "command unconsumed");
    assert_eq!(engine.intent_admitted, admitted);
    assert_eq!(engine.pending_intent, pending);
    assert_eq!(engine.intent_conflicts, conflicts);
    assert_eq!(engine.intent_sticky_blocks, blocks);
    assert_eq!(engine.restored_floats, restored);
    assert!(is_tiled(&engine, "win-a") && is_tiled(&engine, "win-b"));
    // The same save command still works once the report fits.
    let retry = toggle_report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    let reply = accepted(&mut engine, &retry);
    assert_save(&reply, "win-a", &float_true());
}

#[test]
fn reply_bound_target_lanes_rejected_without_mutation() {
    // Pre-mutation REPLY bound refusal driven by the outbound target-lane
    // reservation (not the request-size gate): the report itself fits, but
    // the worst-case reply does not. The padding is tuned so the bound
    // WITHOUT the per-published-window lane term still fits while the
    // bound WITH it exceeds the cap, proving the new reservation is the
    // discriminator. The engine then refuses before any mutation and the
    // same inventory still accepts once the report fits.
    use crate::world_report::WORLD_MAX_REPLY_BYTES;
    use crate::world_sizing::MAX_TARGET_LANES_PER_WINDOW;
    use crate::world_sizing_v4::{
        reply_upper_bound_v4 as reply_upper_bound, report_estimate_v4 as report_estimate,
    };
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let logical = engine.engine.domain_keys().len();
    let staged = engine.staged.facts.len() + engine.staged.held_windows.len();
    let retained = engine.retained_publish_footprint();
    let lanes_of = |windows_len: usize, held_len: usize| {
        windows_len
            .saturating_add(held_len)
            .saturating_add(staged)
            .saturating_add(retained)
            .saturating_mul(MAX_TARGET_LANES_PER_WINDOW)
    };
    let bound_of = |windows: &[crate::world_report::WindowFact]| {
        let report = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.to_vec(),
            vec![],
            "",
            None,
            None,
        );
        reply_upper_bound(&report, 0, logical, 1, staged, retained)
    };
    // Coarse: drop small pad windows until the laneless bound fits.
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) - lanes_of(windows.len(), 0) > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    // Fine: grow one bounded title on an existing pad window - each ASCII
    // title byte adds exactly one to estimate and bound alike (plus 2 for
    // the null-to-string swap: absent titles count `null` as 4 while a
    // string counts its quotes) - landing the laneless bound within one
    // lane term below the cap.
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(111)
        .saturating_sub(bound_of(&windows) - lanes_of(windows.len(), 0))
        + 2;
    windows[2].title = Some("t".repeat(deficit));
    let report = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        None,
        None,
    );
    let lanes = lanes_of(report.windows.len(), report.held_windows.len());
    let without_lanes = reply_upper_bound(&report, 0, logical, 1, staged, retained) - lanes;
    assert!(
        report_estimate(&report) <= crate::world_report::WORLD_MAX_REQUEST_BYTES,
        "request itself fits; only the reply bound trips"
    );
    assert!(
        without_lanes <= WORLD_MAX_REPLY_BYTES,
        "laneless bound fits: {without_lanes}"
    );
    assert!(
        without_lanes + lanes > WORLD_MAX_REPLY_BYTES,
        "lane term is the discriminator: {without_lanes} + {lanes}"
    );
    let revision = engine.state_revision;
    match engine.handle_report(&report) {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REPLY_SIZE);
        }
        other => panic!("expected reply-oversize, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision, "refusal mutates nothing");
    assert!(engine.command_highwater.is_none(), "command unconsumed");
    assert!(is_tiled(&engine, "win-a") && is_tiled(&engine, "win-b"));
    // The same inventory still accepts once the report fits.
    let retry = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        vec![],
        "",
        None,
        None,
    );
    match engine.handle_report(&retry) {
        WorldOutcome::Accepted(_) | WorldOutcome::CommandConsumed(_) => {}
        other => panic!("expected accepted retry, got {other:?}"),
    }
}
