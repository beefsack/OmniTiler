//! Focused lifetime-fenced intent restore: opaque records with
//! `stickyPrior:null` restore explicit float/tile only before first
//! successful admission. Restore reports themselves emit no saves; a
//! successful runtime ToggleFloat in the same or a later report emits the
//! canonical save (see `world_intent_save_tests`).

use crate::boundary::CoreCommand;
use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::intent_data::{IntentData, encode_intent_data};
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{IntentRecord, WorldCommand, WorldReportV4};
use crate::world_v4_support_tests::{accepted, fact, report_full, screen};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
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

#[test]
fn restore_float_true_floats_at_native_frame_without_ops() {
    let mut engine = engine();
    let native = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let report = with_records(
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            None,
            None,
        ),
        vec![record("win-a", &float_true())],
    );
    let reply = accepted(&mut engine, &report);
    // Q4: valid import initializes retained desire and projects it.
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
    let row = row_for(&reply, "win-a").expect("float row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    assert!(reply.effects.iter().all(|e| e.target.0 != "win-a"));
}

#[test]
fn restore_tile_false_nonfixed_survives_hints_and_exclusion() {
    let mut engine = engine();
    // Non-fixed origin restores as an explicit tile with a durable override.
    let report = with_records(
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
        vec![record("win-a", &tile_false())],
    );
    let reply = accepted(&mut engine, &report);
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned())),
        "explicit tile must commit a real override"
    );
    // Hint change to fixed must not re-float an explicit tile.
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fixed_fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    // Retained desire projects on every reply, even without new records.
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(
        is_tiled(&engine, "win-a"),
        "hint change must keep explicit tile"
    );

    // Temporary safety exclusion retires the slot but must not lose intent.
    let mut excluded = fact("win-a", "out-a", 0);
    excluded.owned = Some(true);
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), excluded],
            vec![],
            "",
            None,
            None,
        ),
    );
    // Recovery with fixed hints must tile, not automatic-float.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), fixed_fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(
        is_tiled(&engine, "win-a"),
        "exclusion recovery must keep explicit tile"
    );
    let row = row_for(&reply, "win-a").expect("tiled row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
}

#[test]
fn pending_survives_held_and_gap_then_admits() {
    let mut engine = engine();
    // Record arrives while the fact row is unreadable (held newcomer).
    let mut held = fact("win-a", "out-a", 0);
    held.title = Some("x".repeat(crate::world_report::MAX_METADATA_TEXT_BYTES + 1));
    let mut gapped = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![held],
        vec![],
        "",
        None,
        None,
    );
    gapped.inventory_complete = false;
    let gapped = with_records(gapped, vec![record("win-a", &tile_false())]);
    let reply = accepted(&mut engine, &gapped);
    // Held bootstrap stages pending without projecting yet.
    assert!(reply.intent_desired.is_empty());
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );

    // Readable repeat admits from the retained pending record.
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
}

#[test]
fn first_admission_without_record_ignores_late_record() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(is_tiled(&engine, "win-a"));
    // Late float record after admission confers nothing.
    let f = fact("win-a", "out-a", 0);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &float_true())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(is_tiled(&engine, "win-a"), "late record must not refloat");
}

#[test]
fn same_report_command_supersedes_restore() {
    let mut engine = engine();
    // Restore tiles, then the same-report explicit float flips it.
    let mut report = with_records(
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(1),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-a".to_owned(),
                float_rect: None,
            })),
        ),
        vec![record("win-a", &tile_false())],
    );
    // Command sequence must be fresh; report helper leaves request ids
    // deterministic per sequence.
    report.request_seq = 1;
    let reply = accepted(&mut engine, &report);
    assert_eq!(reply.intent_desired.len(), 1);
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data.as_ref(),
        Some(&IntentData::new(Some(true), None))
    );
    assert!(
        is_floated(&engine, "win-a"),
        "command must supersede restore"
    );
}

#[test]
fn invalid_records_confer_no_authority() {
    for data in [
        "not-json".to_owned(),
        "{\"v\":2,\"float\":true,\"stickyPrior\":null}".to_owned(),
    ] {
        let mut engine = engine();
        let reply = accepted(
            &mut engine,
            &with_records(
                report_full(
                    1,
                    1,
                    vec![screen("out-a", 0)],
                    vec![fixed_fact("win-a", "out-a", 0)],
                    vec![],
                    "",
                    None,
                    None,
                ),
                vec![record("win-a", &data)],
            ),
        );
        assert!(reply.intent_desired.is_empty(), "{data}");
        // Fixed hints with no usable override float automatically.
        assert!(is_floated(&engine, "win-a"), "{data}");
        assert!(engine.pending_intent.is_empty(), "{data}");
        assert!(engine.intent_conflicts.is_empty(), "{data}");
    }
}

#[test]
fn valid_null_and_sticky_records_initialize_retained_desire() {
    // Q4: valid accepted imports initialize the retained desire owner,
    // even under fixed hints (hints alter classification, not storage
    // authority). Malformed/version-invalid above stay empty; these project.
    for data in [
        encode_intent_data(&IntentData::new(None, None)),
        encode_intent_data(&IntentData::new(Some(true), Some(false))),
        encode_intent_data(&IntentData::new(Some(false), Some(true))),
    ] {
        let mut engine = engine();
        let reply = accepted(
            &mut engine,
            &with_records(
                report_full(
                    1,
                    1,
                    vec![screen("out-a", 0)],
                    vec![fixed_fact("win-a", "out-a", 0)],
                    vec![],
                    "",
                    None,
                    None,
                ),
                vec![record("win-a", &data)],
            ),
        );
        assert_eq!(reply.intent_desired.len(), 1, "{data}");
        assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
        let decoded = crate::intent_data::decode_intent_data(&data).expect("valid");
        assert_eq!(reply.intent_desired[0].data, Some(decoded), "{data}");
        // Fixed hints still float; the retained desire is independent.
        assert!(is_floated(&engine, "win-a"), "{data}");
        assert!(engine.intent_conflicts.is_empty(), "{data}");
    }
}

#[test]
fn conflicting_duplicates_confer_no_winner() {
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fixed_fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![
                record("win-a", &float_true()),
                record("win-a", &tile_false()),
            ],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    // Conflict: fixed hints float automatically, never an explicit tile win.
    assert!(is_floated(&engine, "win-a"));
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
    // A later valid record must not resurrect a winner.
    let reply = accepted(
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
            vec![record("win-a", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn pre_first_fact_incomplete_record_admits_on_complete() {
    let mut engine = engine();
    // Record for an absent subject under an incomplete inventory: pending.
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    report.inventory_complete = false;
    let reply = accepted(
        &mut engine,
        &with_records(report, vec![record("win-a", &float_true())]),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );

    // Complete arrival with a usable fact admits as floated.
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    });
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
}

#[test]
fn own_process_record_restores_nothing() {
    let mut engine = engine();
    let mut own = fact("win-self", "out-a", 40);
    own.process_id = Some("test-own-pid".to_owned());
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-t", "out-a", 0), own],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-self", &float_true())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        !engine
            .members
            .contains_key(&WindowId("win-self".to_owned()))
    );
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-self"));
}

#[test]
fn close_and_complete_departure_prune() {
    let mut engine = engine();
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    report.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(report, vec![record("win-a", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );

    // Explicit close drops pending without storing.
    let mut closed = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    closed.closed = vec![WindowId("win-a".to_owned())];
    let closed = with_records(closed, vec![record("win-a", &tile_false())]);
    let reply = accepted(&mut engine, &closed);
    assert!(reply.intent_desired.is_empty());
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );

    // Complete departure of an unadmitted pending subject prunes it.
    let mut report = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    report.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(report, vec![record("win-z", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-z".to_owned()))
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    // The complete departure prunes the ledger; the reply publishes
    // nothing for the proven-departed target.
    assert!(reply.intent_desired.is_empty(), "{reply:?}");
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-z".to_owned()))
    );
    assert!(engine.intent_desired.is_empty(), "pruned on departure");
}

#[test]
fn duplicate_and_oversize_are_non_mutating() {
    let mut engine = engine();
    let first = with_records(
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
        vec![record("win-a", &tile_false())],
    );
    let reply = accepted(&mut engine, &first);
    let revision = reply.state_revision;
    assert!(is_tiled(&engine, "win-a"));

    // Identical latest duplicate returns the cached reply with no revision.
    match engine.handle_report(&first, &[]).0 {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached.state_revision, revision),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert!(is_tiled(&engine, "win-a"));

    // Reused request sequence with different input refuses without mutation.
    let mut reused = with_records(
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
        vec![],
    );
    reused.request_id = "req-1".to_owned();
    assert!(matches!(
        engine.handle_report(&reused, &[]).0,
        WorldOutcome::Rejected { .. }
    ));
    assert!(is_tiled(&engine, "win-a"));
    assert!(!is_tiled(&engine, "win-b"));

    // Oversize request refuses before any mutation.
    let mut big = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        vec![],
        "",
        None,
        None,
    );
    big.screens = vec![screen("out-a", 0); 10_000];
    assert!(matches!(
        engine.handle_report(&big, &[]).0,
        WorldOutcome::Rejected { .. }
    ));
    assert!(is_tiled(&engine, "win-a"));

    // A fresh generation resets the ledger.
    let fresh = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    assert!(fresh.pending_intent.is_empty());
    assert!(fresh.intent_conflicts.is_empty());
    assert!(fresh.intent_admitted.is_empty());
}

#[test]
fn complete_record_only_orphan_never_retains() {
    let mut engine = engine();
    // Complete inventory carrying a record but no fact proves no
    // lifetime: the reply publishes nothing and the footprint-free prune
    // drops it, so no ledger retains it.
    let reply = accepted(
        &mut engine,
        &with_records(
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
            vec![record("win-a", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty(), "{reply:?}");
    assert!(engine.pending_intent.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert!(engine.intent_desired.is_empty(), "pruned footprint-free");
    assert!(!is_tiled(&engine, "win-a"));

    // Incomplete inventory retains the orphan; a complete departure prunes it.
    let mut report = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    report.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(report, vec![record("win-b", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-b".to_owned()))
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(engine.pending_intent.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert!(engine.intent_admitted.is_empty());
    assert!(engine.restored_floats.is_empty());
}

#[test]
fn retained_desire_discriminator_live_publishes_departed_never_does() {
    // Discriminator for the retained owner projection: a live target
    // (fact present) publishes its retained row, while a target with
    // proven complete departure (record but no fact, hold, exclusion,
    // or knowledge) publishes nothing.
    let mut live = engine();
    let reply = accepted(
        &mut live,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 100)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &tile_false())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );

    let mut departed = engine();
    let reply = accepted(
        &mut departed,
        &with_records(
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
            vec![record("win-a", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty(), "{reply:?}");
    assert!(departed.pending_intent.is_empty());
    assert!(departed.intent_conflicts.is_empty());
    assert!(departed.intent_desired.is_empty(), "pruned footprint-free");
}

#[test]
fn restored_true_float_durable_through_second_fact_and_exclusion() {
    let mut engine = engine();
    let native = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &float_true())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );

    // Second ordinary fact: stays floated at the native frame, no ops/writes.
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    let row = row_for(&reply, "win-a").expect("float row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");

    // Automatic fixed float without a record never enters the explicit set.
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![
                {
                    let mut f = fact("win-a", "out-a", 100);
                    f.rect = Some(native);
                    f.transient = Some(false);
                    f
                },
                fixed_fact("win-auto", "out-a", 400),
            ],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-auto"));
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-auto".to_owned()))
    );

    // Safety exclusion retires the slot; recovery keeps the explicit float.
    let mut excluded = fact("win-a", "out-a", 100);
    excluded.rect = Some(native);
    excluded.transient = Some(false);
    excluded.owned = Some(true);
    accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), excluded],
            vec![],
            "",
            None,
            None,
        ),
    );
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &report_full(
            5,
            5,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), f],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn restored_false_then_user_float_clears_override_and_survives_exclusion() {
    let mut engine = engine();
    let reply = accepted(
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
            vec![record("win-a", &tile_false())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // User float supersedes the restored tile and clears the win.
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            Some(2),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-a".to_owned(),
                float_rect: None,
            })),
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1);
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data.as_ref(),
        Some(&IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );

    // Exclusion gap then recovery keeps the user float, never re-tiles.
    let mut excluded = fact("win-a", "out-a", 0);
    excluded.owned = Some(true);
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), excluded],
            vec![],
            "",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn restored_false_minimized_then_fixed_stays_tiled() {
    let mut engine = engine();
    // Pending survives a minimized placeholder without tiling or winning.
    let mut incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(incomplete, vec![record("win-a", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    let mut minimized = fact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![minimized],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Readable arrival admits tiled with a durable override.
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Minimized-then-fixed keeps the explicit tile, never automatic-floats.
    let mut minimized = fact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    accepted(
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
    let reply = accepted(
        &mut engine,
        &report_full(
            5,
            5,
            vec![screen("out-a", 0)],
            vec![fixed_fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn own_process_prunes_pending_and_conflict_clears_on_admission() {
    let mut engine = engine();
    // Pending staged while the subject is still absent.
    let mut incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(incomplete, vec![record("win-self", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-self".to_owned()))
    );
    // Proven-own fact drops pending without admitting or storing.
    let mut own = fact("win-self", "out-a", 40);
    own.process_id = Some("test-own-pid".to_owned());
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-t", "out-a", 0), own],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-self", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-self".to_owned()))
    );
    assert!(!is_tiled(&engine, "win-self"));
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-self"));

    // Conflict confers no winner; admission clears the conflict ledger.
    let mut incomplete = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(
            incomplete,
            vec![
                record("win-c", &float_true()),
                record("win-c", &tile_false()),
            ],
        ),
    );
    assert!(
        engine
            .intent_conflicts
            .contains(&WindowId("win-c".to_owned()))
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-c", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-c".to_owned()))
    );
    // Late valid record after admission confers nothing.
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                5,
                5,
                vec![screen("out-a", 0)],
                vec![fact("win-c", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-c", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-c".to_owned()))
    );
}

#[test]
fn restored_true_unfloat_clears_and_late_record_stays_ignored() {
    let mut engine = engine();
    let native = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &float_true())],
        ),
    );
    assert!(is_floated(&engine, "win-a"));

    // User unfloat tiles and clears the explicit-float provenance.
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            Some(2),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-a".to_owned(),
                float_rect: None,
            })),
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1);
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data.as_ref(),
        Some(&IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );

    // Late true record after admission stays ignored.
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &float_true())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn hints_null_record_still_admits_and_joint_bound_holds() {
    let mut engine = engine();
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    });
    f.transient = Some(false);
    f.hints = None;
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &float_true())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-a"));

    // Joint pending/conflict storage never exceeds the contract bound.
    let mut bounded = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    for i in 0..crate::world_report::MAX_INTENT_RECORDS {
        bounded.pending_intent.insert(
            WindowId(format!("win-{i:05}")),
            crate::intent_data::IntentData::new(Some(false), None),
        );
    }
    assert_eq!(
        bounded.pending_intent.len(),
        crate::world_report::MAX_INTENT_RECORDS
    );
    let reply = accepted(
        &mut bounded,
        &with_records(
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
            vec![record("win-new", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        bounded.pending_intent.len() + bounded.intent_conflicts.len()
            <= crate::world_report::MAX_INTENT_RECORDS
    );
    assert!(
        !bounded
            .pending_intent
            .contains_key(&WindowId("win-new".to_owned()))
    );
}

#[test]
fn rehandshake_resets_intent_ledgers() {
    let mut engine = engine();
    let mut incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(incomplete, vec![record("win-a", &tile_false())]),
    );
    assert!(!engine.pending_intent.is_empty());
    let reply = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "handshake-2".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: Some("example.app".to_owned()),
            executable: Some("example".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: crate::connection::HostCapabilities {
            host_owned: true,
            equal_count: true,
            shared_native_roster: false,
            birth_clear: true,
            native_selector_scope: crate::connection::NativeSelectorScope::AllOutputs,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            warp: false,
            tiled_float_transfer: true,
            unavailable_facts: Vec::new(),
        },
    });
    assert!(reply.kind.is_none());
    assert!(engine.pending_intent.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert!(engine.intent_admitted.is_empty());
    assert!(engine.restored_floats.is_empty());
}

#[test]
fn floating_pending_false_survives_tiling_enable_as_explicit_tile() {
    let mut engine = engine();
    // Healthy setup subject; capture the workspace id.
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    let ws = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .expect("out-a workspace");
    // Switch to floating BEFORE the new subject first appears.
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0)],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: crate::directional::OutputId("out-a".to_owned()),
                workspace: ws.clone(),
                tiled: false,
            }),
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        crate::world_report::TilingMode::Floating
    );
    // Pending false arrives with the first floating-mode fact: explicit
    // tile base retained, never forced tiled while floating.
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-t", "out-a", 0), fact("win-n", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-n", &tile_false())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-n".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(!is_tiled(&engine, "win-n"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-n".to_owned())),
        "floating mode retains the explicit tile base without tiling"
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-n".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-n".to_owned()))
    );
    let row = row_for(&reply, "win-n").expect("floating row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");

    // Enabling tiling with fixed hints keeps the explicit tile override.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), fixed_fact("win-n", "out-a", 400)],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SetWorkspaceTiling {
                output: crate::directional::OutputId("out-a".to_owned()),
                workspace: ws.clone(),
                tiled: true,
            }),
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-n".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-n"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-n".to_owned()))
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-n".to_owned()))
    );
}

#[test]
fn floating_pending_true_stays_intentional_across_tiling_changes() {
    let mut engine = engine();
    let native = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    let ws = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .expect("out-a workspace");
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0)],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: crate::directional::OutputId("out-a".to_owned()),
                workspace: ws.clone(),
                tiled: false,
            }),
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        crate::world_report::TilingMode::Floating
    );
    // Pending true arrives with the first floating-mode fact: explicit
    // float at the observed frame, no geometry op.
    let mut f = fact("win-n", "out-a", 400);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-t", "out-a", 0), f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-n", &float_true())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-n".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(
        engine.pending_intent.is_empty(),
        "floating success consumes pending true"
    );
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-n".to_owned()))
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-n".to_owned()))
    );
    assert!(
        engine
            .members
            .get(&WindowId("win-n".to_owned()))
            .is_some_and(|state| state.floating)
    );
    let row = row_for(&reply, "win-n").expect("floating row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    assert!(reply.effects.iter().all(|e| e.target.0 != "win-n"));

    // Enabling tiling keeps the intentional float with no geometry op.
    let mut f = fact("win-n", "out-a", 400);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), f],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SetWorkspaceTiling {
                output: crate::directional::OutputId("out-a".to_owned()),
                workspace: ws.clone(),
                tiled: true,
            }),
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-n".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(true), None))
    );
    assert!(is_floated(&engine, "win-n"));
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-n".to_owned()))
    );
    let row = row_for(&reply, "win-n").expect("float row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    assert!(reply.effects.iter().all(|e| e.target.0 != "win-n"));
}

#[test]
fn hints_null_twice_without_record_then_first_admissible_imports() {
    let mut engine = engine();
    // First hints-null report without a record: held, member but never latched.
    let mut f = fact("win-a", "out-a", 0);
    f.hints = None;
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(!is_tiled(&engine, "win-a"));
    assert!(engine.pending_intent.is_empty());
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-a".to_owned()))
    );
    // Second hints-null report: still no admission latch from membership alone.
    let mut f = fact("win-a", "out-a", 0);
    f.hints = None;
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-a".to_owned())),
        "held membership alone must never latch admission"
    );
    // First admissible report with a valid false record imports it.
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &tile_false())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-a".to_owned()))
    );
    assert!(engine.pending_intent.is_empty());
}

#[test]
fn floating_workspace_first_admission_ignores_late_record() {
    let mut engine = engine();
    // Healthy setup subject; capture the workspace id.
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    let ws = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .expect("out-a workspace");
    // Switch the workspace to floating BEFORE win-new first appears.
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: crate::directional::OutputId("out-a".to_owned()),
                workspace: ws.clone(),
                tiled: false,
            }),
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        crate::world_report::TilingMode::Floating
    );
    // Brand-new subject's first admission happens in floating mode, record-free.
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-new".to_owned())),
        "floating-mode first admission still latches the fence; admitted={:?} pending={:?} members={:?}",
        engine.intent_admitted,
        engine.pending_intent.keys().collect::<Vec<_>>(),
        engine.members.keys().collect::<Vec<_>>(),
    );
    // Late valid tile record for win-new confers nothing: no pending, no win.
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                4,
                4,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-new", &tile_false())],
        ),
    );
    assert!(reply.intent_desired.is_empty());
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-new".to_owned())),
        "late record must not enter pending"
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-new".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-new".to_owned()))
    );
}

#[test]
fn pending_false_exclusion_gains_fixed_hints_then_recovers_tiled() {
    let mut engine = engine();
    // Pending false with no fact yet.
    let mut incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(incomplete, vec![record("win-a", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );

    // Safety exclusion retires the slot before first success: pending held.
    let mut excluded = fact("win-a", "out-a", 0);
    excluded.owned = Some(true);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), excluded],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Excluded subject gains fixed hints while excluded: still no win.
    let mut excluded_fixed = fixed_fact("win-a", "out-a", 0);
    excluded_fixed.owned = Some(true);
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), excluded_fixed],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Recovery with fixed hints tiles through the retained explicit intent.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), fixed_fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn pending_false_hints_null_and_omission_then_success() {
    let mut engine = engine();
    // Pending false with no fact yet; no premature win.
    let mut incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(
        &mut engine,
        &with_records(incomplete, vec![record("win-a", &tile_false())]),
    );
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Incomplete omission retains pending with no win.
    let mut omitted = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    omitted.inventory_complete = false;
    let reply = accepted(&mut engine, &omitted);
    assert!(reply.intent_desired.is_empty());
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Hints-null fact holds the ordinary newcomer: pending retained, no win.
    let mut f = fact("win-a", "out-a", 0);
    f.hints = None;
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![f],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(!is_tiled(&engine, "win-a"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );

    // Later readable fact succeeds as explicit tile.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn same_report_true_restore_with_unfloat_then_late_ignored() {
    let mut engine = engine();
    let native = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    // Same report restores float and immediately unfloats via command.
    let mut report = with_records(
        report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![f, fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(1),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-a".to_owned(),
                float_rect: None,
            })),
        ),
        vec![record("win-a", &float_true())],
    );
    report.request_seq = 1;
    let reply = accepted(&mut engine, &report);
    assert_eq!(reply.intent_desired.len(), 1);
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data.as_ref(),
        Some(&IntentData::new(Some(false), None))
    );
    assert!(
        is_tiled(&engine, "win-a"),
        "same-report unfloat must supersede the restore"
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );

    // Late true record cannot undo the unfloat.
    let mut f = fact("win-a", "out-a", 100);
    f.rect = Some(native);
    f.transient = Some(false);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![f],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &float_true())],
        ),
    );
    assert_eq!(reply.intent_desired.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_desired[0].data,
        Some(IntentData::new(Some(false), None))
    );
    assert!(is_tiled(&engine, "win-a"));
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
}
