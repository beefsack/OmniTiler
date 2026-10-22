#![forbid(unsafe_code)]
use super::*;

#[test]
fn same_axis_move_schema_v1_missing_field_defaults_to_group() {
    use tiler_windows::settings::{SAME_AXIS_MOVE_GROUP, validate_settings};
    // Pre-item-3 schema-v1 file without the additive field: serde
    // missing-default supplies the group wrap, schema stays version 1.
    let temp = Temp::new("same-axis-missing");
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"inner_gap":8,"outer_gap":8},"bindings":{}}"#,
    )
    .expect("write");
    let LoadOutcome::Loaded(settings) = load_from_dir(&temp.path) else {
        panic!("old file loads");
    };
    assert_eq!(settings.v, 1);
    assert_eq!(settings.core.same_axis_move, SAME_AXIS_MOVE_GROUP);
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn border_radius_default_six_and_flows_to_tile_options() {
    use tiler_windows::settings::{tile_options_from_settings, validate_settings};
    // Windows-only default (2026-10-11); other border defaults unchanged.
    let settings = Settings::default();
    assert_eq!(settings.core.border.width, 3.0);
    assert_eq!(settings.core.border.gap, 0.0);
    assert_eq!(settings.core.border.radius, 6.0);
    assert!(validate_settings(&settings).is_ok());
    // The saved-default base reaches the owner unchanged (Apply path).
    let options = tile_options_from_settings(&settings);
    assert_eq!(options.border.style.width, 3.0);
    assert_eq!(options.border.style.gap, 0.0);
    assert_eq!(options.border.style.radius, 6.0);
}

#[test]
fn border_radius_missing_field_defaults_to_six() {
    use tiler_windows::settings::validate_settings;
    let temp = Temp::new("border-radius-missing");
    // Old file without any border section: serde defaults supply radius 6.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"inner_gap":8,"outer_gap":8},"bindings":{}}"#,
    )
    .expect("write");
    let LoadOutcome::Loaded(settings) = load_from_dir(&temp.path) else {
        panic!("old file loads");
    };
    assert_eq!(settings.core.border.radius, 6.0);
    assert!(validate_settings(&settings).is_ok());
    // Old file with an explicit border section but no radius key: explicit
    // width/gap stay, missing radius takes the new default.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"border":{"width":4.5,"gap":2.0}},"bindings":{}}"#,
    )
    .expect("write");
    let LoadOutcome::Loaded(settings) = load_from_dir(&temp.path) else {
        panic!("old file loads");
    };
    assert_eq!(settings.core.border.width, 4.5);
    assert_eq!(settings.core.border.gap, 2.0);
    assert_eq!(settings.core.border.radius, 6.0);
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn border_radius_explicit_values_survive_reload() {
    // Stored explicit radii remain: save/load round-trips 0 and a custom
    // value without rewriting them to the default.
    let temp = Temp::new("border-radius-explicit");
    for radius in [0.0, 2.5, 6.0] {
        let mut settings = Settings::default();
        settings.core.border.radius = radius;
        save_to_dir(&temp.path, &mut settings).expect("save");
        let LoadOutcome::Loaded(back) = load_from_dir(&temp.path) else {
            panic!("reloads radius {radius}");
        };
        assert_eq!(back.core.border.radius, radius);
    }
}

#[test]
fn border_radius_reload_discards_unsaved_edits() {
    use tiler_windows::settings::tile_options_from_settings;
    // Revert semantics: reloading the saved file discards unsaved in-memory
    // edits and restores the saved radius; nothing is written.
    let temp = Temp::new("border-radius-revert");
    let mut settings = Settings::default();
    save_to_dir(&temp.path, &mut settings).expect("save");
    let saved_bytes = std::fs::read(temp.file()).expect("saved bytes");
    settings.core.border.radius = 0.0;
    let LoadOutcome::Loaded(reverted) = load_from_dir(&temp.path) else {
        panic!("reloads");
    };
    assert_eq!(reverted.core.border.radius, 6.0);
    assert_eq!(
        tile_options_from_settings(&reverted).border.style.radius,
        6.0
    );
    assert_eq!(std::fs::read(temp.file()).expect("bytes"), saved_bytes);
}

#[test]
fn same_axis_move_both_tokens_round_trip() {
    use tiler_windows::settings::{SAME_AXIS_MOVE_GROUP, SAME_AXIS_MOVE_SWAP};
    let temp = Temp::new("same-axis-roundtrip");
    for token in [SAME_AXIS_MOVE_GROUP, SAME_AXIS_MOVE_SWAP] {
        let mut settings = Settings::default();
        settings.core.same_axis_move = token.to_owned();
        save_to_dir(&temp.path, &mut settings).expect("save");
        let LoadOutcome::Loaded(back) = load_from_dir(&temp.path) else {
            panic!("reloads {token}");
        };
        assert_eq!(back.core.same_axis_move, token);
    }
    // Atomic save leaves only the final name behind.
    let entries: Vec<String> = std::fs::read_dir(&temp.path)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["settings.json".to_owned()]);
}

#[test]
fn same_axis_move_invalid_value_and_type_refuse() {
    use tiler_windows::settings::SettingsError;
    let temp = Temp::new("same-axis-invalid");
    // Unknown value (including retired aliases): invalid, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"same_axis_move":"cosmic-wrap"},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Invalid(_))
    ));
    // Wrong JSON type: malformed, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"same_axis_move":7},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Malformed)
    ));
    // Live poll keeps last-good with a degraded status; the file is never
    // rewritten, so a later valid save still applies.
    let mut live_settings = Settings::default();
    live_settings.core.same_axis_move = tiler_windows::settings::SAME_AXIS_MOVE_SWAP.to_owned();
    let live = tiler_windows::settings::LiveSettings::fresh(live_settings, None);
    let outcome = poll_for_change(&temp.path, &live);
    assert!(
        matches!(outcome, PollOutcome::InvalidKept(_)),
        "keeps last-good, got {outcome:?}"
    );
    assert_eq!(live.settings.core.same_axis_move, "swap-with-neighbor");
    let mut valid = Settings::default();
    valid.core.same_axis_move = tiler_windows::settings::SAME_AXIS_MOVE_SWAP.to_owned();
    save_to_dir(&temp.path, &mut valid).expect_err("invalid on-disk file blocks save");
}

#[test]
fn same_axis_move_presets_leave_core_field_unchanged() {
    use tiler_windows::settings::{SAME_AXIS_MOVE_SWAP, validate_settings};
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
        tiler_windows::settings::apply_preset(&mut settings, preset);
        assert_eq!(settings.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
        assert!(validate_settings(&settings).is_ok());
    }
}

#[test]
fn migration_refill_schema_v1_missing_field_defaults_to_last_remaining() {
    use tiler_windows::settings::{MIGRATION_REFILL_LAST_REMAINING, validate_settings};
    // Pre-item-19 schema-v1 file without the additive field: serde
    // missing-default supplies last-remaining-workspace, schema stays v1.
    let temp = Temp::new("migration-refill-missing");
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"inner_gap":8,"outer_gap":8},"bindings":{}}"#,
    )
    .expect("write");
    let LoadOutcome::Loaded(settings) = load_from_dir(&temp.path) else {
        panic!("old file loads");
    };
    assert_eq!(settings.v, 1);
    assert_eq!(
        settings.core.migration_source_refill,
        MIGRATION_REFILL_LAST_REMAINING
    );
    assert_eq!(
        settings.core.migration_source_refill_mode(),
        tiler_core::output_selection::MigrationSourceRefill::LastRemaining
    );
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn migration_refill_both_tokens_round_trip() {
    use tiler_windows::settings::{MIGRATION_REFILL_LAST_REMAINING, MIGRATION_REFILL_MRU};
    let temp = Temp::new("migration-refill-roundtrip");
    for token in [MIGRATION_REFILL_LAST_REMAINING, MIGRATION_REFILL_MRU] {
        let mut settings = Settings::default();
        settings.core.migration_source_refill = token.to_owned();
        save_to_dir(&temp.path, &mut settings).expect("save");
        let LoadOutcome::Loaded(back) = load_from_dir(&temp.path) else {
            panic!("reloads {token}");
        };
        assert_eq!(back.core.migration_source_refill, token);
    }
    // Atomic save leaves only the final name behind.
    let entries: Vec<String> = std::fs::read_dir(&temp.path)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["settings.json".to_owned()]);
}

#[test]
fn migration_refill_invalid_value_and_type_refuse() {
    use tiler_windows::settings::SettingsError;
    let temp = Temp::new("migration-refill-invalid");
    // Unknown value (including near-miss aliases): invalid, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"migration_source_refill":"mru"},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Invalid(_))
    ));
    // Wrong JSON type: malformed, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"migration_source_refill":7},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Malformed)
    ));
    // Live poll keeps last-good with a degraded status; the file is never
    // rewritten, so a later valid save still applies.
    let mut live_settings = Settings::default();
    live_settings.core.migration_source_refill =
        tiler_windows::settings::MIGRATION_REFILL_MRU.to_owned();
    let live = tiler_windows::settings::LiveSettings::fresh(live_settings, None);
    let outcome = poll_for_change(&temp.path, &live);
    assert!(
        matches!(outcome, PollOutcome::InvalidKept(_)),
        "keeps last-good, got {outcome:?}"
    );
    assert_eq!(
        live.settings.core.migration_source_refill,
        "most-recently-used-workspace"
    );
    let mut valid = Settings::default();
    valid.core.migration_source_refill = tiler_windows::settings::MIGRATION_REFILL_MRU.to_owned();
    save_to_dir(&temp.path, &mut valid).expect_err("invalid on-disk file blocks save");
}

#[test]
fn migration_refill_presets_leave_core_field_unchanged() {
    use tiler_windows::settings::{MIGRATION_REFILL_MRU, validate_settings};
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.migration_source_refill = MIGRATION_REFILL_MRU.to_owned();
        tiler_windows::settings::apply_preset(&mut settings, preset);
        assert_eq!(settings.core.migration_source_refill, MIGRATION_REFILL_MRU);
        assert!(validate_settings(&settings).is_ok());
    }
}

// Item 14 catalog/settings: four follow-only migration rows (one per
// direction), all bindable unbound with Keep meaning unbound under both
// presets; a rebound rides the exact Win+Ctrl+Alt+Shift arm into the
// canonical letter slot through the migrate action.

#[test]
fn catalog_exposes_migration_rows_unbound_with_exact_modifiers() {
    use tiler_windows::settings::{
        binding_action, binding_canonical_vk, binding_catalog, binding_wants_alt,
        binding_wants_ctrl, binding_wants_shift,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    assert_eq!(catalog.len(), 103);
    for (id, canon) in [
        ("migrate-workspace-left", 0x48u32),
        ("migrate-workspace-right", 0x4Cu32),
        ("migrate-workspace-up", 0x4Bu32),
        ("migrate-workspace-down", 0x4Au32),
    ] {
        let def = catalog.iter().find(|def| def.id == id).expect("row");
        assert!(def.defaults.is_empty(), "{id} unbound by default");
        assert!(def.implemented, "{id}");
        assert!(def.conflict.is_none(), "{id} carries no invented holder");
        assert!(binding_wants_shift(def), "{id}");
        assert!(binding_wants_ctrl(def), "{id}");
        assert!(binding_wants_alt(def), "{id}");
        assert_eq!(binding_action(def), ChordAction::WorkspaceMigrate);
        assert_eq!(binding_canonical_vk(def), Some(canon), "{id} letter slot");
        assert_eq!(
            tiler_windows::settings::binding_arm_text(def),
            "Win+Ctrl+Alt+Shift",
            "{id}"
        );
    }
}

#[test]
fn migration_rows_stay_unbound_under_both_presets() {
    use tiler_windows::settings::{apply_preset, effective_bindings};
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        let changed = apply_preset(&mut settings, preset);
        for id in [
            "migrate-workspace-left",
            "migrate-workspace-right",
            "migrate-workspace-up",
            "migrate-workspace-down",
        ] {
            assert!(!changed.contains(&id), "{id} keeps unbound {preset:?}");
            assert!(!settings.bindings.contains_key(id), "{id} {preset:?}");
        }
        for row in effective_bindings(&settings) {
            if row.id.starts_with("migrate-workspace-") {
                assert!(row.active, "{}", row.id);
                assert!(row.chords.is_empty(), "{}", row.id);
                assert!(!row.effective, "{}", row.id);
                assert_eq!(row.reason, "unbound: bindable", "{}", row.id);
            }
        }
        assert!(tiler_windows::settings::validate_settings(&settings).is_ok());
    }
}

#[test]
fn migration_rebind_routes_into_canonical_slot_with_full_modifiers() {
    use tiler_windows::settings::{
        BindingSetting, BindingState, binding_modifiers_ok, build_disabled, build_remap,
        validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = tiler_windows::settings::binding_catalog();
    let def = catalog
        .iter()
        .find(|def| def.id == "migrate-workspace-left")
        .expect("row");
    // Exact full-modifier arm only: shifted Ctrl+Alt without Win-Ctrl-Alt-Shift
    // shape refuses (missing Alt here), other arms refuse too.
    assert!(binding_modifiers_ok(def, true, true, true));
    assert!(!binding_modifiers_ok(def, false, true, true));
    assert!(!binding_modifiers_ok(def, true, false, true));
    assert!(!binding_modifiers_ok(def, true, true, false));
    assert!(!binding_modifiers_ok(def, false, false, false));
    // A rebound onto an arrow chord routes into the canonical letter slot
    // through the migrate action; the old (empty) default suppresses nothing.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "migrate-workspace-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Alt+Shift+Left".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(remap[0].from_vk, tiler_windows::snapkey::VK_LEFT);
    assert!(remap[0].from_shift && remap[0].from_ctrl && remap[0].from_alt);
    assert_eq!(remap[0].action, ChordAction::WorkspaceMigrate);
    assert_eq!(remap[0].to_vk, 0x48);
    assert!(build_disabled(&settings).is_empty());
    // A duplicate physical chord on another row still refuses.
    settings.bindings.insert(
        "migrate-workspace-right".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Alt+Shift+Left".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
}

#[test]
fn born_maximized_schema_v1_missing_field_defaults_to_tile() {
    use tiler_windows::settings::{BORN_MAXIMIZED_TILE, validate_settings};
    // Pre-R-MAX-06 schema-v1 file without the additive field: serde
    // missing-default supplies tile-on-launch, schema stays version 1.
    let temp = Temp::new("born-max-missing");
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"inner_gap":8,"outer_gap":8},"bindings":{}}"#,
    )
    .expect("write");
    let LoadOutcome::Loaded(settings) = load_from_dir(&temp.path) else {
        panic!("old file loads");
    };
    assert_eq!(settings.v, 1);
    assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_TILE);
    assert!(!settings.core.born_maximized_keep_overlay());
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn born_maximized_both_tokens_round_trip() {
    use tiler_windows::settings::{BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE};
    let temp = Temp::new("born-max-roundtrip");
    for token in [BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_OVERLAY] {
        let mut settings = Settings::default();
        settings.core.born_maximized = token.to_owned();
        save_to_dir(&temp.path, &mut settings).expect("save");
        let LoadOutcome::Loaded(back) = load_from_dir(&temp.path) else {
            panic!("reloads {token}");
        };
        assert_eq!(back.core.born_maximized, token);
    }
    // Atomic save leaves only the final name behind.
    let entries: Vec<String> = std::fs::read_dir(&temp.path)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["settings.json".to_owned()]);
}

#[test]
fn born_maximized_invalid_value_and_type_refuse() {
    use tiler_windows::settings::SettingsError;
    let temp = Temp::new("born-max-invalid");
    // Unknown value (including retired aliases): invalid, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"born_maximized":"keep-overlay"},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Invalid(_))
    ));
    // Wrong JSON type: malformed, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"born_maximized":7},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Malformed)
    ));
    // Live poll keeps last-good with a degraded status; the file is never
    // rewritten, so a later valid save still applies.
    let mut live_settings = Settings::default();
    live_settings.core.born_maximized = tiler_windows::settings::BORN_MAXIMIZED_OVERLAY.to_owned();
    let live = tiler_windows::settings::LiveSettings::fresh(live_settings, None);
    let outcome = poll_for_change(&temp.path, &live);
    assert!(
        matches!(outcome, PollOutcome::InvalidKept(_)),
        "keeps last-good, got {outcome:?}"
    );
    assert_eq!(live.settings.core.born_maximized, "keep-maximized-overlay");
    let mut valid = Settings::default();
    valid.core.born_maximized = tiler_windows::settings::BORN_MAXIMIZED_OVERLAY.to_owned();
    save_to_dir(&temp.path, &mut valid).expect_err("invalid on-disk file blocks save");
}

#[test]
fn born_maximized_presets_leave_core_field_unchanged() {
    use tiler_windows::settings::{BORN_MAXIMIZED_OVERLAY, validate_settings};
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
        tiler_windows::settings::apply_preset(&mut settings, preset);
        assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
        assert!(validate_settings(&settings).is_ok());
    }
}
// Stage 2c cursor-first targeting: additive schema-v1 fields with strict
// tokens, file-level backfill, invalid-file refusal with last-good kept,
// and no preset coupling.

#[test]
fn cursor_targeting_schema_v1_missing_fields_default_cursor_off() {
    use tiler_windows::settings::{
        FRESH_PLACEMENT_CURSOR_MRU, SWITCH_TARGET_CURSOR, validate_settings,
    };
    // Older schema-v1 file without the additive fields: serde
    // missing-defaults supply cursor-output/MRU plus off/off, schema v1.
    let temp = Temp::new("cursor-targeting-missing");
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"inner_gap":8,"outer_gap":8},"bindings":{}}"#,
    )
    .expect("write");
    let LoadOutcome::Loaded(settings) = load_from_dir(&temp.path) else {
        panic!("old file loads");
    };
    assert_eq!(settings.v, 1);
    assert_eq!(settings.core.switch_target, SWITCH_TARGET_CURSOR);
    assert_eq!(
        settings.core.switch_target_mode(),
        tiler_core::cursor_targeting::SwitchTarget::Cursor
    );
    assert_eq!(settings.core.fresh_placement, FRESH_PLACEMENT_CURSOR_MRU);
    assert_eq!(
        settings.core.fresh_placement_mode(),
        tiler_core::cursor_targeting::FreshPlacement::CursorOutputMru
    );
    assert!(!settings.core.focus_follows_cursor);
    assert!(!settings.core.cursor_warp);
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn cursor_targeting_tokens_round_trip() {
    use tiler_windows::settings::{
        FRESH_PLACEMENT_CURSOR_CLOSEST, FRESH_PLACEMENT_FOCUSED_MRU, SWITCH_TARGET_FOCUSED,
    };
    let temp = Temp::new("cursor-targeting-roundtrip");
    for (switch, placement, ffc, warp) in [
        (
            SWITCH_TARGET_FOCUSED,
            FRESH_PLACEMENT_FOCUSED_MRU,
            true,
            true,
        ),
        (
            tiler_windows::settings::SWITCH_TARGET_CURSOR,
            FRESH_PLACEMENT_CURSOR_CLOSEST,
            false,
            true,
        ),
    ] {
        let mut settings = Settings::default();
        settings.core.switch_target = switch.to_owned();
        settings.core.fresh_placement = placement.to_owned();
        settings.core.focus_follows_cursor = ffc;
        settings.core.cursor_warp = warp;
        save_to_dir(&temp.path, &mut settings).expect("save");
        let LoadOutcome::Loaded(back) = load_from_dir(&temp.path) else {
            panic!("reloads {switch}/{placement}");
        };
        assert_eq!(back.core.switch_target, switch);
        assert_eq!(back.core.fresh_placement, placement);
        assert_eq!(back.core.focus_follows_cursor, ffc);
        assert_eq!(back.core.cursor_warp, warp);
    }
    // Atomic save leaves only the final name behind.
    let entries: Vec<String> = std::fs::read_dir(&temp.path)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["settings.json".to_owned()]);
}

#[test]
fn cursor_targeting_invalid_value_and_type_refuse() {
    use tiler_windows::settings::SettingsError;
    let temp = Temp::new("cursor-targeting-invalid");
    // Unknown values (including near-miss aliases): invalid, bytes untouched.
    for (key, bad) in [
        ("switch_target", "focused"),
        ("switch_target", ""),
        ("fresh_placement", "mru"),
        ("fresh_placement", ""),
    ] {
        let body = format!(
            "{{\"v\":1,\"revision\":1,\"core\":{{\"{key}\":\"{bad}\"}},\"bindings\":{{}}}}"
        );
        std::fs::write(temp.file(), body).expect("write");
        assert!(
            matches!(
                load_from_dir(&temp.path),
                LoadOutcome::Invalid(SettingsError::Invalid(_))
            ),
            "{key}={bad}"
        );
    }
    // Wrong JSON type: malformed, bytes untouched.
    std::fs::write(
        temp.file(),
        br#"{"v":1,"revision":1,"core":{"switch_target":7},"bindings":{}}"#,
    )
    .expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(SettingsError::Malformed)
    ));
    // Live poll keeps last-good with a degraded status; the file is never
    // rewritten, so a later valid save still applies.
    let mut live_settings = Settings::default();
    live_settings.core.switch_target = "focused".to_owned();
    let live = tiler_windows::settings::LiveSettings::fresh(live_settings, None);
    let outcome = poll_for_change(&temp.path, &live);
    assert!(
        matches!(outcome, PollOutcome::InvalidKept(_)),
        "keeps last-good, got {outcome:?}"
    );
    assert_eq!(live.settings.core.switch_target, "focused");
    let mut valid = Settings::default();
    save_to_dir(&temp.path, &mut valid).expect_err("invalid on-disk file blocks save");
}

#[test]
fn cursor_targeting_presets_leave_core_fields_unchanged() {
    use tiler_windows::settings::{SWITCH_TARGET_FOCUSED, validate_settings};
    // No all-settings presets exist: binding presets never touch these
    // lanes (or any core lane).
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.switch_target = SWITCH_TARGET_FOCUSED.to_owned();
        settings.core.fresh_placement =
            tiler_windows::settings::FRESH_PLACEMENT_FOCUSED_MRU.to_owned();
        settings.core.focus_follows_cursor = true;
        settings.core.cursor_warp = true;
        tiler_windows::settings::apply_preset(&mut settings, preset);
        assert_eq!(settings.core.switch_target, SWITCH_TARGET_FOCUSED);
        assert_eq!(
            settings.core.fresh_placement,
            tiler_windows::settings::FRESH_PLACEMENT_FOCUSED_MRU
        );
        assert!(settings.core.focus_follows_cursor);
        assert!(settings.core.cursor_warp);
        assert!(validate_settings(&settings).is_ok());
    }
}
