//! Focused restore-only semantic sticky intent consumption: valid whole
//! sticky-bearing records enter pending intent before first admission, and a
//! real healthy float success commits the exact sticky prior into lifetime
//! provenance (never into ordinary float provenance or a tile override).

use crate::boundary::CoreCommand;
use crate::directional::WindowId;
use crate::intent_data::{IntentData, encode_intent_data};
use crate::world_engine::WorldEngine;
use crate::world_report::{IntentRecord, WorldCommand, WorldReportV4};
use crate::world_v4_support_tests::{accepted, fact, float_fact, rect, report_full, screen};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn engine_with(native_sticky: bool) -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky,
            // Fixtures carry native maximize facts; keep the maximize lane
            // supported so the sticky lane under test stays isolated.
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn toggle(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    })
}

fn row_for<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    win: &str,
) -> Option<&'a crate::world_report::WindowDesire> {
    reply.windows.iter().find(|row| row.lifetime.0 == win)
}

fn rehandshake(engine: &mut WorldEngine) -> String {
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

fn record(win: &str, data: &str) -> IntentRecord {
    IntentRecord {
        win: WindowId(win.to_owned()),
        data: data.to_owned(),
    }
}

fn with_records(mut report: WorldReportV4, records: Vec<IntentRecord>) -> WorldReportV4 {
    report.intent_records = records;
    report
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

/// Fresh restart admission of one saved sticky import through the record
/// lane. Shared by the restart tests so each case stays a few lines.
fn restart_with_import(
    saved: &IntentData,
    sticky: Option<bool>,
) -> (WorldEngine, crate::world_report::WorldReplyV4) {
    let mut fresh = engine();
    let mut restart_fact = fact("win-a", "out-a", 0);
    restart_fact.rect = Some(rect(100, 100, 200, 200));
    restart_fact.sticky = sticky;
    let reply = accepted(
        &mut fresh,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![restart_fact],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &encode_intent_data(saved))],
        ),
    );
    (fresh, reply)
}

fn restart_binding<'a>(
    engine: &'a WorldEngine,
    window: &str,
) -> &'a crate::world_state::RetainedStateBinding {
    engine
        .retained_states
        .get(&(
            WindowId(window.to_owned()),
            crate::world_report::StateProperty::Sticky,
        ))
        .expect("restored binding")
}

#[test]
fn all_six_sticky_bearing_records_ingest_whole_into_pending() {
    let mut engine = engine();
    let combos: [(Option<bool>, bool, &str); 6] = [
        (None, false, "win-null-false"),
        (None, true, "win-null-true"),
        (Some(false), false, "win-false-false"),
        (Some(false), true, "win-false-true"),
        (Some(true), false, "win-true-false"),
        (Some(true), true, "win-true-true"),
    ];
    let facts: Vec<_> = combos
        .iter()
        .map(|(_, _, win)| fact(win, "out-a", 0))
        .collect();
    let records: Vec<_> = combos
        .iter()
        .map(|(float, prior, win)| {
            record(
                win,
                &encode_intent_data(&IntentData::new(*float, Some(*prior))),
            )
        })
        .collect();
    // Gapped inventory: ingest only, no admission, so pending stays
    // observable whole.
    let mut gapped = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        facts,
        vec![],
        "",
        None,
        None,
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &with_records(gapped, records));
    assert!(reply.intent_writes.is_empty());
    for (float, prior, win) in &combos {
        let id = WindowId(win.to_string());
        assert_eq!(
            engine.pending_intent.get(&id).copied(),
            Some(IntentData::new(*float, Some(*prior))),
            "{win}"
        );
        assert!(
            engine.intent_sticky_blocks.contains(&id),
            "sticky scan must block saves for {win}"
        );
    }
    assert!(engine.intent_conflicts.is_empty());
}

#[test]
fn sticky_dominates_float_lane_and_commits_exact_prior_without_ops() {
    let mut engine = engine();
    let native = rect(100, 100, 200, 200);
    let mut fa = fact("win-a", "out-a", 100);
    fa.rect = Some(native);
    let mut fb = fact("win-b", "out-a", 300);
    fb.rect = Some(native);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fa, fb],
                vec![],
                "",
                None,
                None,
            ),
            vec![
                // Sticky dominates the tile lane: must float, never tile.
                record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(Some(false), Some(true))),
                ),
                record(
                    "win-b",
                    &encode_intent_data(&IntentData::new(None, Some(false))),
                ),
            ],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    for win in ["win-a", "win-b"] {
        assert!(is_floated(&engine, win), "{win} must ride sticky float");
        assert!(!is_tiled(&engine, win), "{win} must never tile");
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .expect("float row");
        assert!(row.frame_operation.is_none(), "{row:?}");
        assert!(row.clear_operation.is_none(), "{row:?}");
    }
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true),
        "prior must stay the exact bool"
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-b".to_owned())),
        Some(&false),
        "prior must stay the exact bool"
    );
    assert!(engine.restored_floats.is_empty(), "never ordinary float");
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-a".to_owned())),
        "sticky float:false must never commit a tile override"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn sticky_provenance_survives_record_free_report_and_prunes_on_close() {
    let mut engine = engine();
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
            vec![record(
                "win-a",
                &encode_intent_data(&IntentData::new(Some(true), Some(false))),
            )],
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    // Record-free report: provenance and membership survive with no writes.
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
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-a"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    // Explicit close prunes the lifetime.
    let mut closed = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        Vec::new(),
        vec![],
        "",
        None,
        None,
    );
    closed.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut engine, &closed);
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
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn all_six_sticky_priors_admit_as_semantic_float_with_exact_prior() {
    // Every `float` lane (null/false/true) with a non-null prior restores
    // as a sticky float and commits the exact prior bool: never a tile
    // override, never ordinary float provenance, no geometry or save
    // effects during import.
    let combos: [(Option<bool>, bool); 6] = [
        (None, false),
        (None, true),
        (Some(false), false),
        (Some(false), true),
        (Some(true), false),
        (Some(true), true),
    ];
    for (float, prior) in combos {
        let mut engine = engine();
        let native = rect(100, 100, 200, 200);
        let mut f = fact("win-a", "out-a", 100);
        f.rect = Some(native);
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
                vec![record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(float, Some(prior))),
                )],
            ),
        );
        assert!(
            reply.intent_writes.is_empty(),
            "float={float:?} prior={prior}"
        );
        assert!(
            is_floated(&engine, "win-a"),
            "float={float:?} prior={prior}"
        );
        assert!(!is_tiled(&engine, "win-a"), "float={float:?} prior={prior}");
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&prior),
            "float={float:?} prior={prior}"
        );
        assert!(
            engine.restored_floats.is_empty(),
            "float={float:?} prior={prior}"
        );
        assert!(
            !engine
                .engine
                .has_fixed_tile_override_any(&WindowId("win-a".to_owned())),
            "float={float:?} prior={prior}"
        );
        assert!(
            engine.pending_intent.is_empty(),
            "float={float:?} prior={prior}"
        );
        assert!(
            engine.intent_conflicts.is_empty(),
            "float={float:?} prior={prior}"
        );
        let row = row_for(&reply, "win-a").expect("float row");
        assert!(row.frame_operation.is_none(), "{row:?}");
        assert!(row.clear_operation.is_none(), "{row:?}");
        assert!(
            reply.effects.iter().all(|e| e.target.0 != "win-a"),
            "import mints no focus: {reply:?}"
        );
    }
}

#[test]
fn native_false_composed_null_tiles_but_sticky_record_forces_float() {
    // Composed route (native_sticky=false): raw null tiles ordinarily.
    let mut engine = engine_with(false);
    let mut composed = fact("win-c", "out-a", 0);
    composed.sticky = None;
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![composed],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_tiled(&engine, "win-c"));

    // The same composed null with a sticky-bearing record restores as a
    // semantic sticky float with the exact prior.
    let mut engine = engine_with(false);
    let mut composed = fact("win-c", "out-a", 0);
    composed.sticky = None;
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![composed],
                vec![],
                "",
                None,
                None,
            ),
            vec![record(
                "win-c",
                &encode_intent_data(&IntentData::new(None, Some(true))),
            )],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-c"));
    assert!(!is_tiled(&engine, "win-c"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-c".to_owned())),
        Some(&true)
    );
    assert!(engine.restored_floats.is_empty());
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("win-c".to_owned()))
    );
}

#[test]
fn native_false_nonnull_fact_with_sticky_record_stays_fail_closed() {
    // Unsupported raw non-null under native_sticky=false holds narrowly
    // even when a sticky record is present: no slot, no admission, no
    // float/tile, pending retained behind the block.
    let mut engine = engine_with(false);
    let mut conflicted = fact("win-x", "out-a", 0);
    conflicted.sticky = Some(true);
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![conflicted],
                vec![],
                "",
                None,
                None,
            ),
            vec![record(
                "win-x",
                &encode_intent_data(&IntentData::new(Some(false), Some(false))),
            )],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(!is_floated(&engine, "win-x"));
    assert!(!is_tiled(&engine, "win-x"));
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-x"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-x".to_owned())),
        "unsupported hold retains pending sticky"
    );
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-x".to_owned()))
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-x".to_owned()))
    );
    assert!(
        !engine.members.keys().any(|id| id.0 == "win-x"),
        "conflicted subject takes no membership"
    );
    // Composed-null recovery consumes the retained sticky and floats with
    // the exact prior, minting no save.
    let mut recovered = fact("win-x", "out-a", 0);
    recovered.sticky = None;
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![recovered],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-x"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-x".to_owned())),
        Some(&false)
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-x".to_owned()))
    );
}

#[test]
fn native_true_null_holds_pending_and_recovers_record_free() {
    // Supported null is an unknown sticky input: semantic intent must not
    // bypass it. Pending stays, nothing admits or floats.
    let mut engine = engine_with(true);
    let mut unknown = fact("win-u", "out-a", 0);
    unknown.sticky = None;
    let reply = accepted(
        &mut engine,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![unknown],
                vec![],
                "",
                None,
                None,
            ),
            vec![record(
                "win-u",
                &encode_intent_data(&IntentData::new(Some(true), Some(true))),
            )],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(!is_floated(&engine, "win-u"));
    assert!(!is_tiled(&engine, "win-u"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-u".to_owned())),
        "supported null holds sticky classification"
    );
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-u".to_owned()))
    );
    // Known-bool arrival needs no record: the retained pending consumes
    // into the exact prior and floats.
    let mut known = fact("win-u", "out-a", 0);
    known.sticky = Some(false);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![known],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-u"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-u".to_owned())),
        Some(&true),
        "prior stays exact, not the raw fact"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-u".to_owned()))
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-u".to_owned()))
    );
}

#[test]
fn sticky_pending_survives_unhealthy_shapes_then_admits() {
    // Held, minimized, and excluded subjects retain pending sticky without
    // committing; a hints-null fact still admits because semantic sticky
    // proves float without constraint facts; a newcomer absent under an
    // incomplete inventory retains pending until complete arrival.
    let sticky = encode_intent_data(&IntentData::new(Some(false), Some(true)));

    // Held newcomer (overbound title).
    {
        let mut engine = engine();
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
        let reply = accepted(
            &mut engine,
            &with_records(gapped, vec![record("win-a", &sticky)]),
        );
        assert!(reply.intent_writes.is_empty());
        assert!(!is_floated(&engine, "win-a"));
        assert!(
            engine
                .pending_intent
                .contains_key(&WindowId("win-a".to_owned()))
        );
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
        assert!(reply.intent_writes.is_empty());
        assert!(is_floated(&engine, "win-a"));
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&true)
        );
    }

    // Minimized placeholder retains pending.
    {
        let mut engine = engine();
        let mut minimized = fact("win-a", "out-a", 0);
        minimized.minimized = Some(true);
        let mut gapped = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![minimized],
            vec![],
            "",
            None,
            None,
        );
        gapped.inventory_complete = false;
        let reply = accepted(
            &mut engine,
            &with_records(gapped, vec![record("win-a", &sticky)]),
        );
        assert!(reply.intent_writes.is_empty());
        assert!(
            engine
                .pending_intent
                .contains_key(&WindowId("win-a".to_owned()))
        );
        assert!(
            !engine
                .restored_sticky
                .contains_key(&WindowId("win-a".to_owned()))
        );
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
        assert!(reply.intent_writes.is_empty());
        assert!(is_floated(&engine, "win-a"));
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&true)
        );
    }

    // Safety exclusion retires the slot but keeps pending and the block.
    {
        let mut engine = engine();
        let mut excluded = fact("win-a", "out-a", 0);
        excluded.owned = Some(true);
        let mut gapped = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 400), excluded],
            vec![],
            "",
            None,
            None,
        );
        gapped.inventory_complete = false;
        let reply = accepted(
            &mut engine,
            &with_records(gapped, vec![record("win-a", &sticky)]),
        );
        assert!(reply.intent_writes.is_empty());
        assert!(!is_floated(&engine, "win-a"));
        assert!(
            engine
                .pending_intent
                .contains_key(&WindowId("win-a".to_owned()))
        );
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-t", "out-a", 400), fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
        );
        assert!(reply.intent_writes.is_empty());
        assert!(is_floated(&engine, "win-a"));
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&true)
        );
    }

    // Hints-null with sticky intent still admits as a sticky float.
    {
        let mut engine = engine();
        let mut unreadable = fact("win-a", "out-a", 100);
        unreadable.rect = Some(rect(100, 100, 200, 200));
        unreadable.hints = None;
        let reply = accepted(
            &mut engine,
            &with_records(
                report_full(
                    1,
                    1,
                    vec![screen("out-a", 0)],
                    vec![unreadable],
                    vec![],
                    "",
                    None,
                    None,
                ),
                vec![record("win-a", &sticky)],
            ),
        );
        assert!(reply.intent_writes.is_empty());
        assert!(is_floated(&engine, "win-a"));
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&true)
        );
    }

    // Newcomer absent under incomplete inventory retains, then admits.
    {
        let mut engine = engine();
        let mut gapped = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        );
        gapped.inventory_complete = false;
        let reply = accepted(
            &mut engine,
            &with_records(gapped, vec![record("win-a", &sticky)]),
        );
        assert!(reply.intent_writes.is_empty());
        assert!(
            engine
                .pending_intent
                .contains_key(&WindowId("win-a".to_owned()))
        );
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
        assert!(reply.intent_writes.is_empty());
        assert!(is_floated(&engine, "win-a"));
        assert_eq!(
            engine.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&true)
        );
    }
}

#[test]
fn sticky_late_and_conflicting_records_stay_ignored_after_admission() {
    let mut engine = engine();
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
            vec![record(
                "win-a",
                &encode_intent_data(&IntentData::new(Some(true), Some(false))),
            )],
        ),
    );
    assert!(is_floated(&engine, "win-a"));
    // Late differing record for the admitted lifetime is ignored: no
    // pending, no conflict, block and provenance intact.
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
            vec![record(
                "win-a",
                &encode_intent_data(&IntentData::new(Some(false), Some(true))),
            )],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-a"));
    assert!(engine.pending_intent.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    // Same-report duplicate conflict after admission is likewise ignored.
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
            vec![
                record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(Some(true), Some(false))),
                ),
                record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(Some(true), Some(true))),
                ),
            ],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-a"));
    assert!(engine.pending_intent.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
}

#[test]
fn sticky_orphan_pre_ready_and_close_own_rehandshake_cleanup() {
    // Pre-ready incomplete orphan retains pending and the block.
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
        vec![record(
            "win-x",
            &encode_intent_data(&IntentData::new(None, Some(true))),
        )],
    );
    pre.inventory_complete = false;
    let boot = accepted(&mut engine, &pre);
    assert!(!boot.ready, "incomplete first report bootstraps");
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-x".to_owned()))
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-x".to_owned()))
    );
    // Complete inventory without the subject prunes the footprint-free
    // orphan entirely.
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
            .pending_intent
            .contains_key(&WindowId("win-x".to_owned()))
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-x".to_owned()))
    );
    assert!(
        !engine
            .intent_conflicts
            .contains(&WindowId("win-x".to_owned()))
    );

    // Own-process proof prunes a pre-admission sticky pending and block.
    let mut gapped = with_records(
        report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
        vec![record(
            "win-a",
            &encode_intent_data(&IntentData::new(Some(true), Some(false))),
        )],
    );
    gapped.inventory_complete = false;
    accepted(&mut engine, &gapped);
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    let mut own = fact("win-a", "out-a", 0);
    own.process_id = Some("test-own-pid".to_owned());
    accepted(
        &mut engine,
        &report_full(
            4,
            4,
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
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );

    // Admitted sticky then explicit close prunes provenance and the block.
    accepted(
        &mut engine,
        &with_records(
            report_full(
                5,
                5,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record(
                "win-a",
                &encode_intent_data(&IntentData::new(Some(true), Some(false))),
            )],
        ),
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    let mut closed = report_full(
        6,
        6,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    closed.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut engine, &closed);
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-a".to_owned()))
    );

    // Rehandshake resets every sticky ledger, including the saturation
    // latch.
    accepted(
        &mut engine,
        &with_records(
            report_full(
                7,
                7,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
            vec![record(
                "win-a",
                &encode_intent_data(&IntentData::new(Some(true), Some(false))),
            )],
        ),
    );
    assert!(
        engine
            .intent_sticky_blocks
            .contains(&WindowId("win-a".to_owned()))
    );
    let _ = rehandshake(&mut engine);
    assert!(engine.pending_intent.is_empty());
    assert!(engine.intent_conflicts.is_empty());
    assert!(engine.intent_admitted.is_empty());
    assert!(engine.restored_sticky.is_empty());
    assert!(engine.restored_floats.is_empty());
    assert!(engine.intent_sticky_blocks.is_empty());
    assert!(!engine.intent_sticky_saturated);
}

#[test]
fn sticky_floating_first_appearance_then_reenable_keeps_provenance() {
    let mut engine = engine();
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
    assert!(reply.intent_writes.is_empty());
    // First appearance in the floating workspace with a sticky record:
    // consumes into exact-prior provenance, never ordinary float.
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
            vec![record(
                "win-n",
                &encode_intent_data(&IntentData::new(Some(false), Some(true))),
            )],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(engine.pending_intent.is_empty());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-n".to_owned())),
        Some(&true)
    );
    assert!(engine.restored_floats.is_empty());
    assert!(
        engine
            .members
            .get(&WindowId("win-n".to_owned()))
            .is_some_and(|state| state.floating)
    );
    let row = row_for(&reply, "win-n").expect("floating row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    // Re-enabling tiling keeps the sticky float with no geometry op.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), fact("win-n", "out-a", 400)],
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
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&engine, "win-n"));
    assert!(!is_tiled(&engine, "win-n"));
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-n".to_owned())),
        Some(&true)
    );
    let row = row_for(&reply, "win-n").expect("float row");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
}

#[test]
fn sticky_joint_ledger_commit_steady_and_overflow_fail_closed() {
    // Pending-to-retained sticky commit holds the joint count steady:
    // pending leaves first, so block plus provenance occupy the same
    // slots.
    let mut engine = engine();
    let mut gapped = with_records(
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
        vec![record(
            "win-a",
            &encode_intent_data(&IntentData::new(Some(true), Some(false))),
        )],
    );
    gapped.inventory_complete = false;
    accepted(&mut engine, &gapped);
    assert_eq!(engine.intent_ledger_used(), 2, "pending plus block");
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
    assert!(reply.intent_writes.is_empty());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    assert_eq!(engine.intent_ledger_used(), 2, "block plus provenance");
    assert!(!engine.intent_sticky_saturated);

    // Full ledger refuses a new sticky insert fail-closed and latches.
    let mut capped = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    for i in 0..crate::world_report::MAX_INTENT_RECORDS {
        capped.pending_intent.insert(
            WindowId(format!("win-{i:05}")),
            IntentData::new(Some(false), None),
        );
    }
    let mut gapped = with_records(
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
        vec![record(
            "win-new",
            &encode_intent_data(&IntentData::new(None, Some(true))),
        )],
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut capped, &gapped);
    assert!(reply.intent_writes.is_empty());
    assert!(capped.intent_sticky_saturated, "overflow latches");
    assert!(
        !capped
            .pending_intent
            .contains_key(&WindowId("win-new".to_owned()))
    );
    assert!(
        !capped
            .intent_sticky_blocks
            .contains(&WindowId("win-new".to_owned()))
    );
    assert_eq!(
        capped.intent_ledger_used(),
        crate::world_report::MAX_INTENT_RECORDS
    );
}

#[test]
fn sticky_toggle_refuses_only_usable_pending_or_restored() {
    // Usable pre-admission pending refuses without mutation.
    {
        let mut local = engine();
        let mut gapped = with_records(
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
            vec![record(
                "win-a",
                &encode_intent_data(&IntentData::new(Some(true), Some(false))),
            )],
        );
        gapped.inventory_complete = false;
        accepted(&mut local, &gapped);
        assert!(
            local
                .pending_intent
                .contains_key(&WindowId("win-a".to_owned()))
        );
        let _revision = local.state_revision;
        let native_clear = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(2),
            Some(toggle("win-a")),
        );
        let reply = accepted(&mut local, &native_clear);
        assert!(reply.command_result.is_none());
        assert_eq!(
            local
                .retained_states
                .get(&(
                    WindowId("win-a".to_owned()),
                    crate::world_report::StateProperty::Sticky
                ))
                .map(|binding| binding.target.value),
            Some(false)
        );
        assert_eq!(local.command_highwater, Some(2));
    }

    // Restored sticky refuses without mutation.
    {
        let mut local = engine();
        accepted(
            &mut local,
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
                vec![record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(Some(true), Some(false))),
                )],
            ),
        );
        assert!(is_floated(&local, "win-a"));
        let native_clear = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(2),
            Some(toggle("win-a")),
        );
        let reply = accepted(&mut local, &native_clear);
        assert!(reply.command_result.is_none());
        assert_eq!(
            local
                .retained_states
                .get(&(
                    WindowId("win-a".to_owned()),
                    crate::world_report::StateProperty::Sticky
                ))
                .map(|binding| binding.target.value),
            Some(false)
        );
        assert_eq!(local.command_highwater, Some(2));
        assert!(is_floated(&local, "win-a"));
    }

    // Conflict plus block without usable intent does not refuse: the
    // ordinary command still runs while publication stays save-blocked.
    {
        let mut local = engine();
        let mut gapped = with_records(
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
                record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(Some(true), Some(false))),
                ),
                record(
                    "win-a",
                    &encode_intent_data(&IntentData::new(Some(false), Some(true))),
                ),
            ],
        );
        gapped.inventory_complete = false;
        accepted(&mut local, &gapped);
        assert!(
            local
                .intent_conflicts
                .contains(&WindowId("win-a".to_owned()))
        );
        assert!(
            local
                .intent_sticky_blocks
                .contains(&WindowId("win-a".to_owned()))
        );
        assert!(
            !local
                .pending_intent
                .contains_key(&WindowId("win-a".to_owned()))
        );
        assert!(
            !local
                .restored_sticky
                .contains_key(&WindowId("win-a".to_owned()))
        );
        let revision = local.state_revision;
        let conflicted_toggle = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(2),
            Some(toggle("win-b")),
        );
        let reply = accepted(&mut local, &conflicted_toggle);
        assert_eq!(
            revision + 1,
            local.state_revision,
            "unrelated toggle still runs"
        );
        assert_eq!(
            reply.intent_writes.len(),
            1,
            "block is per-lifetime: {reply:?}"
        );
        assert_eq!(reply.intent_writes[0].win, WindowId("win-b".to_owned()));
    }
}

#[test]
fn sticky_restart_reacquires_native_and_off_restores_prior() {
    use crate::world_report::{
        FrameSetter, ScopeAuthority, StateAttempt, StateProperty, WorldCommand,
    };

    fn set_sticky(window: &str, value: bool) -> WorldCommand {
        WorldCommand::SetSticky {
            window: WindowId(window.to_owned()),
            value,
        }
    }

    fn sticky_row(
        reply: &crate::world_report::WorldReplyV4,
        window: &str,
    ) -> Option<(bool, ScopeAuthority, Option<u64>)> {
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .and_then(|row| {
                row.state
                    .iter()
                    .find(|target| target.property == StateProperty::Sticky)
                    .map(|target| (target.value, target.authority, target.command_seq))
            })
    }

    for prior in [false, true] {
        // Origin: a real sticky-on command mints the exact saved intent.
        let mut origin = engine();
        if prior {
            accepted(
                &mut origin,
                &report_full(
                    1,
                    1,
                    vec![screen("out-a", 0)],
                    vec![float_fact("win-a", "out-a", 0, 0)],
                    vec![],
                    "",
                    None,
                    None,
                ),
            );
            let mut on_fact = float_fact("win-a", "out-a", 0, 0);
            on_fact.sticky = Some(false);
            let on_reply = accepted(
                &mut origin,
                &report_full(
                    2,
                    2,
                    vec![screen("out-a", 0)],
                    vec![on_fact],
                    vec![],
                    "",
                    Some(1),
                    Some(set_sticky("win-a", true)),
                ),
            );
            assert!(on_reply.command_result.is_none(), "prior={prior}");
            let save = on_reply.intent_writes.first().expect("save").clone();
            assert_eq!(
                crate::intent_data::decode_intent_data(save.data.as_deref().expect("bytes"))
                    .expect("decodes"),
                crate::intent_data::IntentData::new(Some(true), Some(true)),
                "prior={prior}"
            );
            assert_eq!(save.win.0, "win-a");
        } else {
            accepted(
                &mut origin,
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
            let on_reply = accepted(
                &mut origin,
                &report_full(
                    2,
                    2,
                    vec![screen("out-a", 0)],
                    vec![fact("win-a", "out-a", 0)],
                    vec![],
                    "",
                    Some(1),
                    Some(set_sticky("win-a", true)),
                ),
            );
            assert!(on_reply.command_result.is_none(), "prior={prior}");
            let save = on_reply.intent_writes.first().expect("save").clone();
            assert_eq!(
                crate::intent_data::decode_intent_data(save.data.as_deref().expect("bytes"))
                    .expect("decodes"),
                crate::intent_data::IntentData::new(Some(true), Some(false)),
                "prior={prior}"
            );
            assert_eq!(save.win.0, "win-a");
        }
        let saved_data = origin
            .sticky_saved
            .get(&WindowId("win-a".to_owned()))
            .copied()
            .expect("origin saved");
        assert_eq!(saved_data.sticky_prior, Some(prior));

        // Restart: fresh owner, native original preimage (sticky false)
        // restored first, saved intent replays through the record lane.
        let mut fresh = engine();
        let mut restart_fact = fact("win-a", "out-a", 0);
        restart_fact.rect = Some(rect(100, 100, 200, 200));
        restart_fact.sticky = Some(false);
        let saved_bytes = encode_intent_data(&saved_data);
        let restart_reply = accepted(
            &mut fresh,
            &with_records(
                report_full(
                    1,
                    1,
                    vec![screen("out-a", 0)],
                    vec![restart_fact],
                    vec![],
                    "",
                    None,
                    None,
                ),
                vec![record("win-a", &saved_bytes)],
            ),
        );
        assert!(restart_reply.intent_writes.is_empty(), "prior={prior}");
        assert!(is_floated(&fresh, "win-a"), "prior={prior}");
        assert_eq!(
            fresh.restored_sticky.get(&WindowId("win-a".to_owned())),
            Some(&prior),
            "prior={prior}"
        );
        // The restored intent must reacquire native all-desktop membership
        // through an ordinary authorized target, not semantic float alone.
        assert_eq!(
            sticky_row(&restart_reply, "win-a"),
            Some((true, ScopeAuthority::Controlled, None)),
            "prior={prior}: restart must publish native sticky reacquire"
        );
        let binding = fresh
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .expect("restored binding");
        assert!(binding.target.value, "prior={prior}");
        assert!(!binding.spent, "prior={prior}");
        assert_eq!(
            binding.sticky_meta.as_ref().expect("meta").command_seq,
            None,
            "prior={prior}: commandless restart mints no sequence"
        );
        assert_eq!(
            binding
                .sticky_meta
                .as_ref()
                .expect("meta")
                .prior_restored_sticky,
            Some(prior),
            "prior={prior}"
        );
        let rev = binding.target.revision;

        // Adapter performs the authorized write; the correlated evidence
        // settles without a new command or save.
        let mut proof_fact = fact("win-a", "out-a", 0);
        proof_fact.rect = Some(rect(100, 100, 200, 200));
        proof_fact.sticky = Some(true);
        let mut proof = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![proof_fact],
            vec![],
            "",
            None,
            None,
        );
        proof.state_attempts = vec![StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Sticky,
            target: true,
            revision: rev,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 7,
        }];
        let proof_reply = accepted(&mut fresh, &proof);
        assert!(proof_reply.intent_writes.is_empty(), "prior={prior}");
        let settled = fresh
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .expect("binding");
        assert!(settled.spent, "prior={prior}");
        assert_eq!(
            settled.disposition,
            crate::world_report::AttemptOutcome::Applied,
            "prior={prior}"
        );

        // A real sticky-off command restores the exact prior mode.
        let mut off_fact = fact("win-a", "out-a", 0);
        off_fact.rect = Some(rect(100, 100, 200, 200));
        off_fact.sticky = Some(true);
        let off_reply = accepted(
            &mut fresh,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![off_fact],
                vec![],
                "",
                Some(1),
                Some(set_sticky("win-a", false)),
            ),
        );
        assert!(off_reply.command_result.is_none(), "prior={prior}");
        let off_save = off_reply.intent_writes.first().expect("off save").clone();
        assert_eq!(
            crate::intent_data::decode_intent_data(off_save.data.as_deref().expect("bytes"))
                .expect("decodes"),
            crate::intent_data::IntentData::new(Some(prior), None),
            "prior={prior}"
        );
        let off_rev = fresh
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .expect("off binding")
            .target
            .revision;
        assert_eq!(
            fresh
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
                .expect("off binding")
                .sticky_meta
                .as_ref()
                .expect("meta")
                .command_seq,
            Some(1),
            "prior={prior}: real OFF commands keep their sequence"
        );
        let mut off_proof_fact = fact("win-a", "out-a", 0);
        off_proof_fact.sticky = Some(false);
        if prior {
            off_proof_fact.rect = Some(rect(100, 100, 200, 200));
        }
        let mut off_proof = report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![off_proof_fact],
            vec![],
            "",
            None,
            None,
        );
        off_proof.state_attempts = vec![StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Sticky,
            target: false,
            revision: off_rev,
            setter: FrameSetter::Accepted,
            pre: Some(true),
            post: Some(false),
            attempt_ms: 8,
        }];
        accepted(&mut fresh, &off_proof);
        assert!(
            !fresh
                .restored_sticky
                .contains_key(&WindowId("win-a".to_owned())),
            "prior={prior}"
        );
        if prior {
            assert!(is_floated(&fresh, "win-a"), "prior float stays float");
            assert!(!is_tiled(&fresh, "win-a"), "prior float never tiles");
        } else {
            assert!(is_tiled(&fresh, "win-a"), "prior tile fresh-admits tile");
            assert!(!is_floated(&fresh, "win-a"), "prior tile leaves float");
        }
    }
}

#[test]
fn sticky_restart_refusal_preserves_import_without_retry() {
    use crate::world_report::{AttemptOutcome, FrameSetter, StateAttempt, StateProperty};
    let saved = IntentData::new(Some(true), Some(false));
    let (mut fresh, restart_reply) = restart_with_import(&saved, Some(false));
    assert!(restart_reply.intent_writes.is_empty());
    let rev = restart_binding(&fresh, "win-a").target.revision;
    // Exact-tuple refusal: the write never landed, so rollback re-asserts
    // the same import instead of erasing sticky provenance.
    let mut refused_fact = fact("win-a", "out-a", 0);
    refused_fact.rect = Some(rect(101, 100, 200, 200));
    refused_fact.sticky = Some(false);
    let mut refused = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![refused_fact],
        vec![],
        "",
        None,
        None,
    );
    refused.state_attempts = vec![StateAttempt {
        window: WindowId("win-a".to_owned()),
        property: StateProperty::Sticky,
        target: true,
        revision: rev,
        setter: FrameSetter::Refused,
        pre: Some(false),
        post: Some(false),
        attempt_ms: 7,
    }];
    let reply = accepted(&mut fresh, &refused);
    assert_eq!(
        reply.intent_writes,
        vec![crate::world_report::IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&saved)),
        }]
    );
    assert_eq!(
        fresh.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false),
        "proven refusal keeps the import"
    );
    assert_eq!(
        fresh.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&saved)
    );
    let binding = restart_binding(&fresh, "win-a");
    assert!(binding.spent);
    assert_eq!(binding.disposition, AttemptOutcome::RefusedProven);
    // A retried proof under a fresh sequence emits nothing further.
    refused.request_seq = 3;
    refused.observation_seq = 3;
    refused.request_id = "req-3".to_owned();
    let again = accepted(&mut fresh, &refused);
    assert!(again.intent_writes.is_empty(), "one proof, one write");
}

#[test]
fn sticky_restart_ambiguous_evidence_keeps_import_without_retry() {
    use crate::world_report::{AttemptOutcome, FrameSetter, StateAttempt, StateProperty};
    let saved = IntentData::new(Some(true), Some(true));
    let (mut fresh, restart_reply) = restart_with_import(&saved, Some(false));
    assert!(restart_reply.intent_writes.is_empty());
    let rev = restart_binding(&fresh, "win-a").target.revision;
    // Accepted setter whose post misses the target: ambiguous evidence
    // spends the binding but never settles and never retries.
    let mut ambiguous_fact = fact("win-a", "out-a", 0);
    ambiguous_fact.rect = Some(rect(101, 100, 200, 200));
    ambiguous_fact.sticky = Some(false);
    let mut ambiguous = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![ambiguous_fact],
        vec![],
        "",
        None,
        None,
    );
    ambiguous.state_attempts = vec![StateAttempt {
        window: WindowId("win-a".to_owned()),
        property: StateProperty::Sticky,
        target: true,
        revision: rev,
        setter: FrameSetter::Accepted,
        pre: Some(false),
        post: Some(false),
        attempt_ms: 7,
    }];
    let reply = accepted(&mut fresh, &ambiguous);
    assert!(reply.intent_writes.is_empty());
    let binding = restart_binding(&fresh, "win-a");
    assert!(binding.spent);
    assert_eq!(binding.disposition, AttemptOutcome::PossiblyExecuted);
    assert!(
        !binding.sticky_meta.as_ref().expect("meta").followup_done,
        "ambiguous never completes"
    );
    assert_eq!(
        fresh.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true)
    );
    assert_eq!(
        fresh.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&saved)
    );
    // A retried proof under a fresh sequence emits nothing further.
    ambiguous.request_seq = 3;
    ambiguous.observation_seq = 3;
    ambiguous.request_id = "req-3".to_owned();
    let again = accepted(&mut fresh, &ambiguous);
    assert!(again.intent_writes.is_empty(), "ambiguous never retries");
}

#[test]
fn sticky_restart_unknown_holds_then_record_free_recovers() {
    use crate::world_report::StateProperty;
    let saved = IntentData::new(Some(true), Some(true));
    let (mut fresh, reply) = restart_with_import(&saved, None);
    assert!(reply.intent_writes.is_empty());
    assert!(
        fresh
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned())),
        "unknown sticky holds pending"
    );
    assert!(!is_floated(&fresh, "win-a"));
    assert!(
        !fresh
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "held import mints no binding"
    );
    // Record-free known-bool arrival consumes the retained pending into
    // the exact prior and mints the commandless binding.
    let mut known = fact("win-a", "out-a", 0);
    known.rect = Some(rect(100, 100, 200, 200));
    known.sticky = Some(false);
    let reply = accepted(
        &mut fresh,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![known],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert!(is_floated(&fresh, "win-a"));
    assert_eq!(
        fresh.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true),
        "prior stays exact, not the raw fact"
    );
    let binding = restart_binding(&fresh, "win-a");
    assert!(!binding.spent);
    assert_eq!(
        binding.sticky_meta.as_ref().expect("meta").command_seq,
        None
    );
}

#[test]
fn sticky_restart_floating_route_mints_binding_next_observation() {
    use crate::world_report::{ScopeAuthority, StateProperty, WorldCommand};
    // Floating workspace: a new window first appears there, so the
    // finish-lane latch (not the session route) admits the import.
    let mut engine = engine();
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
    let ws = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .expect("workspace");
    accepted(
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
                workspace: ws,
                tiled: false,
            }),
        ),
    );
    let saved = IntentData::new(Some(false), Some(true));
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
            vec![record("win-n", &encode_intent_data(&saved))],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-n".to_owned())),
        Some(&true)
    );
    let binding = engine
        .retained_states
        .get(&(WindowId("win-n".to_owned()), StateProperty::Sticky))
        .expect("floating binding");
    assert_eq!(
        binding.sticky_meta.as_ref().expect("meta").command_seq,
        None
    );
    assert!(
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-n")
            .and_then(|row| row
                .state
                .iter()
                .find(|target| target.property == StateProperty::Sticky))
            .is_none(),
        "floating route lands next observation, not the committing reply"
    );
    let next = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), fact("win-n", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let target = next
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-n")
        .and_then(|row| {
            row.state
                .iter()
                .find(|target| target.property == StateProperty::Sticky)
        })
        .expect("next-observation target");
    assert_eq!(
        (target.value, target.authority, target.command_seq),
        (true, ScopeAuthority::Controlled, None)
    );
}

#[test]
fn sticky_restart_import_shapes_share_binding_policy() {
    use crate::connection::HostCapabilities;
    use crate::world_report::{ScopeAuthority, StateProperty};
    // Already-native-true fact: the write is idempotent, still commandless.
    let saved = IntentData::new(Some(true), Some(true));
    let (fresh, reply) = restart_with_import(&saved, Some(true));
    let target = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .and_then(|row| {
            row.state
                .iter()
                .find(|target| target.property == StateProperty::Sticky)
        })
        .expect("already-true target");
    assert_eq!(
        (target.value, target.authority, target.command_seq),
        (true, ScopeAuthority::Controlled, None)
    );
    assert_eq!(
        restart_binding(&fresh, "win-a")
            .sticky_meta
            .as_ref()
            .expect("meta")
            .command_seq,
        None
    );
    // Composed keeps provenance with no state target. Suppression-capable
    // null facts are the composed admission shape; raw sticky stays null.
    let mut composed = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let mut composed_fact = fact("win-a", "out-a", 0);
    composed_fact.rect = Some(rect(100, 100, 200, 200));
    composed_fact.sticky = None;
    composed_fact.suppression_available = Some(true);
    composed_fact.visibility.suppression_owned = Some(false);
    let reply = accepted(
        &mut composed,
        &with_records(
            report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![composed_fact],
                vec![],
                "",
                None,
                None,
            ),
            vec![record("win-a", &encode_intent_data(&saved))],
        ),
    );
    assert!(reply.intent_writes.is_empty());
    assert_eq!(
        composed.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&true)
    );
    assert!(
        !composed
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "composed mints no state target"
    );
    assert!(
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .and_then(|row| row
                .state
                .iter()
                .find(|target| target.property == StateProperty::Sticky))
            .is_none(),
        "composed publishes no sticky target"
    );
}
