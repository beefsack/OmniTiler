#![forbid(unsafe_code)]
use super::*;

#[test]
fn defaults_match_kde_parity() {
    let settings = Settings::default();
    assert_eq!(settings.v, SETTINGS_SCHEMA_VERSION);
    assert_eq!(settings.core.inner_gap, 8);
    assert_eq!(settings.core.outer_gap, 8);
    assert!(settings.core.border.enabled);
    assert_eq!(settings.core.border.width, 3.0);
    assert_eq!(settings.core.border.gap, 0.0);
    assert_eq!(settings.core.border.radius, 6.0);
    assert_eq!(settings.core.border.color, "#2a82da");
    assert!(settings.core.border.use_theme);
    assert!(settings.core.underlay.enabled);
    assert_eq!(settings.core.underlay.color, "#662a82da");
    assert_eq!(settings.core.underlay.extension, -1.0);
    assert!(settings.core.keyboard.takeover);
    assert!(!settings.core.keyboard.allow_win_l);
    assert!(settings.core.mouse.snap_prevention);
    assert!(settings.core.workspace.default_tiled);
    assert!(settings.core.taskbar_pager);
    assert_eq!(settings.core.same_axis_move, SAME_AXIS_MOVE_GROUP);
    assert_eq!(
        settings.core.same_axis_move_mode(),
        tiler_core::directional::SameAxisMove::GroupWithNeighbor
    );
    assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_TILE);
    assert!(!settings.core.born_maximized_keep_overlay());
    assert_eq!(settings.core.fixed_size_predicate, FIXED_SIZE_BOTH_AXES);
    assert_eq!(
        settings.core.fixed_size_predicate_mode(),
        tiler_core::size_hints::FixedSizePredicate::BothAxes
    );
    assert_eq!(
        settings.core.migration_source_refill,
        MIGRATION_REFILL_LAST_REMAINING
    );
    assert_eq!(
        settings.core.migration_source_refill_mode(),
        tiler_core::output_selection::MigrationSourceRefill::LastRemaining
    );
    assert_eq!(
        settings.core.workspace.mode,
        WORKSPACE_MODE_PER_OUTPUT_LOCAL
    );
    assert_eq!(
        settings.core.workspace.mode_enum(),
        tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal
    );
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn taskbar_pager_defaults_on_backfills_and_persists_opt_out() {
    // Additive schema-v1 field: older files without the key deserialize on
    // and validate; an explicit false round-trips (the opt-out persists).
    assert!(Settings::default().core.taskbar_pager);
    let settings: Settings =
        serde_json::from_str(r#"{"v":1,"core":{}}"#).expect("missing key parses");
    assert!(settings.core.taskbar_pager);
    assert!(validate_settings(&settings).is_ok());
    assert!(crate::pager::pager_enabled(&settings));
    let doc = serde_json::json!({
        "v": SETTINGS_SCHEMA_VERSION,
        "revision": 1,
        "core": {"taskbar_pager": false},
        "bindings": {}
    });
    let settings: Settings = serde_json::from_value(doc).expect("explicit false deserializes");
    assert!(!settings.core.taskbar_pager);
    assert!(validate_settings(&settings).is_ok());
    assert!(!crate::pager::pager_enabled(&settings));
    let round_tripped: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).expect("serializes"))
            .expect("round-trips");
    assert_eq!(round_tripped, settings);
}

#[test]
fn same_axis_move_tokens_map_exactly_with_group_default() {
    assert_eq!(
        tiler_core::directional::SameAxisMove::parse_wire(SAME_AXIS_MOVE_GROUP),
        Some(tiler_core::directional::SameAxisMove::GroupWithNeighbor)
    );
    assert_eq!(
        tiler_core::directional::SameAxisMove::parse_wire(SAME_AXIS_MOVE_SWAP),
        Some(tiler_core::directional::SameAxisMove::SwapWithNeighbor)
    );
    assert_eq!(
        tiler_core::directional::SameAxisMove::GroupWithNeighbor.as_wire_str(),
        SAME_AXIS_MOVE_GROUP
    );
    assert_eq!(
        tiler_core::directional::SameAxisMove::SwapWithNeighbor.as_wire_str(),
        SAME_AXIS_MOVE_SWAP
    );
    // No retired aliases, no case folding, no empty token.
    for bad in [
        "",
        "cosmic-wrap",
        "flat-swap",
        "GROUP-WITH-NEIGHBOR",
        "swap",
    ] {
        assert!(
            tiler_core::directional::SameAxisMove::parse_wire(bad).is_none(),
            "{bad}"
        );
    }
    let mut settings = Settings::default();
    settings.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
    assert_eq!(
        settings.core.same_axis_move_mode(),
        tiler_core::directional::SameAxisMove::SwapWithNeighbor
    );
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn same_axis_move_options_carry_functional_labels_and_wm_tips() {
    assert_eq!(
        same_axis_move_options(),
        [
            (SAME_AXIS_MOVE_GROUP, "Group with neighbor", "COSMIC"),
            (SAME_AXIS_MOVE_SWAP, "Swap with neighbor", "i3, sway"),
        ]
    );
}

#[test]
fn same_axis_move_invalid_value_refuses() {
    let mut settings = Settings::default();
    settings.core.same_axis_move = "cosmic-wrap".to_owned();
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::Invalid(_))
    ));
}

#[test]
fn same_axis_move_presets_leave_core_field_unchanged() {
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
        apply_preset(&mut settings, preset);
        assert_eq!(settings.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
        assert!(validate_settings(&settings).is_ok());
    }
}

#[test]
fn fixed_size_predicate_tokens_map_exactly_with_both_axes_default() {
    use tiler_core::size_hints::FixedSizePredicate::{BothAxes, EitherAxis};
    assert_eq!(
        tiler_core::size_hints::FixedSizePredicate::parse_wire(FIXED_SIZE_BOTH_AXES),
        Some(BothAxes)
    );
    assert_eq!(
        tiler_core::size_hints::FixedSizePredicate::parse_wire(FIXED_SIZE_EITHER_AXIS),
        Some(EitherAxis)
    );
    assert_eq!(BothAxes.as_wire_str(), FIXED_SIZE_BOTH_AXES);
    assert_eq!(EitherAxis.as_wire_str(), FIXED_SIZE_EITHER_AXIS);
    // No aliases, no case folding, no empty token, no retired IDs.
    for bad in [
        "",
        "both",
        "either",
        "BOTH-AXES-FIXED",
        "width-and-height",
        "fixed",
    ] {
        assert!(
            tiler_core::size_hints::FixedSizePredicate::parse_wire(bad).is_none(),
            "{bad}"
        );
    }
    let mut settings = Settings::default();
    settings.core.fixed_size_predicate = FIXED_SIZE_EITHER_AXIS.to_owned();
    assert_eq!(settings.core.fixed_size_predicate_mode(), EitherAxis);
    assert!(validate_settings(&settings).is_ok());
    // Hand-built unknown values fail closed to the default mode but
    // refuse through validation (last-good on disk, never live).
    settings.core.fixed_size_predicate = "either".to_owned();
    assert_eq!(settings.core.fixed_size_predicate_mode(), BothAxes);
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::Invalid(_))
    ));
}

#[test]
fn fixed_size_predicate_options_carry_functional_labels_and_wm_tips() {
    assert_eq!(
        fixed_size_predicate_options(),
        [
            (
                FIXED_SIZE_BOTH_AXES,
                "Width and height both fixed",
                "COSMIC"
            ),
            (
                FIXED_SIZE_EITHER_AXIS,
                "Width or height fixed",
                "Hyprland (Wayland), sway"
            ),
        ]
    );
}

#[test]
fn fixed_size_predicate_presets_leave_core_field_unchanged() {
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.fixed_size_predicate = FIXED_SIZE_EITHER_AXIS.to_owned();
        apply_preset(&mut settings, preset);
        assert_eq!(settings.core.fixed_size_predicate, FIXED_SIZE_EITHER_AXIS);
        assert!(validate_settings(&settings).is_ok());
    }
}

#[test]
fn migration_source_refill_tokens_map_exactly_with_last_remaining_default() {
    use tiler_core::output_selection::MigrationSourceRefill::{LastRemaining, MostRecentlyUsed};
    assert_eq!(
        tiler_core::output_selection::MigrationSourceRefill::parse_wire(
            MIGRATION_REFILL_LAST_REMAINING
        ),
        Some(LastRemaining)
    );
    assert_eq!(
        tiler_core::output_selection::MigrationSourceRefill::parse_wire(MIGRATION_REFILL_MRU),
        Some(MostRecentlyUsed)
    );
    assert_eq!(LastRemaining.as_wire_str(), MIGRATION_REFILL_LAST_REMAINING);
    assert_eq!(MostRecentlyUsed.as_wire_str(), MIGRATION_REFILL_MRU);
    assert_eq!(
        tiler_core::output_selection::MigrationSourceRefill::default(),
        LastRemaining
    );
    // No aliases, no case folding, no empty token, no retired IDs.
    for bad in [
        "",
        "mru",
        "last-remaining",
        "most-recently-used",
        "LAST-REMAINING-WORKSPACE",
        "last_remaining_workspace",
    ] {
        assert!(
            tiler_core::output_selection::MigrationSourceRefill::parse_wire(bad).is_none(),
            "{bad}"
        );
    }
    let mut settings = Settings::default();
    settings.core.migration_source_refill = MIGRATION_REFILL_MRU.to_owned();
    assert_eq!(
        settings.core.migration_source_refill_mode(),
        MostRecentlyUsed
    );
    assert!(validate_settings(&settings).is_ok());
    // Hand-built unknown values fail closed to the default mode but
    // refuse through validation (last-good on disk, never live).
    settings.core.migration_source_refill = "mru".to_owned();
    assert_eq!(settings.core.migration_source_refill_mode(), LastRemaining);
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::Invalid(_))
    ));
}

#[test]
fn migration_source_refill_options_carry_functional_labels_and_wm_tips() {
    assert_eq!(
        migration_source_refill_options(),
        [
            (
                MIGRATION_REFILL_LAST_REMAINING,
                "Last remaining workspace",
                "COSMIC"
            ),
            (
                MIGRATION_REFILL_MRU,
                "Most recently used workspace",
                "bspwm, i3, awesome"
            ),
        ]
    );
}

#[test]
fn migration_source_refill_presets_leave_core_field_unchanged() {
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.migration_source_refill = MIGRATION_REFILL_MRU.to_owned();
        apply_preset(&mut settings, preset);
        assert_eq!(settings.core.migration_source_refill, MIGRATION_REFILL_MRU);
        assert!(validate_settings(&settings).is_ok());
    }
}

#[test]
fn workspace_mode_tokens_map_exactly_with_local_default() {
    use tiler_core::workspace_scope::WorkspaceMode::{GlobalUnique, PerOutputLocal, Shared};
    // Windows wire constants pin the shared tokens and default.
    assert_eq!(
        tiler_core::workspace_scope::mode_token(PerOutputLocal),
        WORKSPACE_MODE_PER_OUTPUT_LOCAL
    );
    assert_eq!(
        tiler_core::workspace_scope::mode_token(GlobalUnique),
        WORKSPACE_MODE_GLOBAL_UNIQUE
    );
    assert_eq!(
        tiler_core::workspace_scope::mode_token(Shared),
        WORKSPACE_MODE_SHARED
    );
    assert_eq!(
        tiler_core::workspace_scope::DEFAULT_WORKSPACE_MODE,
        PerOutputLocal
    );
    assert_eq!(
        tiler_core::workspace_scope::WORKSPACE_MODE_TOKENS,
        [
            WORKSPACE_MODE_PER_OUTPUT_LOCAL,
            WORKSPACE_MODE_GLOBAL_UNIQUE,
            WORKSPACE_MODE_SHARED
        ]
    );
    let mut settings = Settings::default();
    assert_eq!(settings.core.workspace.mode_enum(), PerOutputLocal);
    settings.core.workspace.mode = WORKSPACE_MODE_GLOBAL_UNIQUE.to_owned();
    assert_eq!(settings.core.workspace.mode_enum(), GlobalUnique);
    assert!(validate_settings(&settings).is_ok());
    settings.core.workspace.mode = WORKSPACE_MODE_SHARED.to_owned();
    assert_eq!(settings.core.workspace.mode_enum(), Shared);
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn workspace_mode_options_carry_functional_labels_and_scope_notes() {
    assert_eq!(
        workspace_mode_options(),
        [
            (
                WORKSPACE_MODE_PER_OUTPUT_LOCAL,
                "Per-output local",
                "Each output keeps its own set"
            ),
            (
                WORKSPACE_MODE_GLOBAL_UNIQUE,
                "Global unique",
                "Distinct sets with cross-output swap"
            ),
            (
                WORKSPACE_MODE_SHARED,
                "Shared",
                "One set shown across all outputs"
            ),
        ]
    );
}

#[test]
fn workspace_mode_missing_key_backfills_local_default() {
    // Additive schema-v1 field: older files without the key deserialize to
    // the default and validate; explicit values round-trip unchanged.
    let settings: Settings =
        serde_json::from_str(r#"{"v":1,"core":{"workspace":{}}}"#).expect("missing key parses");
    assert_eq!(
        settings.core.workspace.mode,
        WORKSPACE_MODE_PER_OUTPUT_LOCAL
    );
    assert!(validate_settings(&settings).is_ok());
    let settings: Settings =
        serde_json::from_str(r#"{"v":1,"core":{}}"#).expect("missing section parses");
    assert_eq!(
        settings.core.workspace.mode,
        WORKSPACE_MODE_PER_OUTPUT_LOCAL
    );
    assert!(validate_settings(&settings).is_ok());
    for token in [
        WORKSPACE_MODE_PER_OUTPUT_LOCAL,
        WORKSPACE_MODE_GLOBAL_UNIQUE,
        WORKSPACE_MODE_SHARED,
    ] {
        let doc = serde_json::json!({
            "v": SETTINGS_SCHEMA_VERSION,
            "revision": 1,
            "core": {"workspace": {"mode": token}},
            "bindings": {}
        });
        let settings: Settings = serde_json::from_value(doc).expect("explicit token deserializes");
        assert_eq!(settings.core.workspace.mode, token);
        assert!(validate_settings(&settings).is_ok());
        let round_tripped: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).expect("serializes"))
                .expect("round-trips");
        assert_eq!(round_tripped, settings);
    }
}

#[test]
fn workspace_mode_invalid_value_refuses() {
    // Strict refusal: unknown tokens deserialize but never validate
    // (last-good kept); the typed accessor fails closed to the default.
    for bad in [
        "",
        "local",
        "global",
        "PER-OUTPUT-LOCAL",
        "per_output_local",
        "group-with-neighbor",
    ] {
        let mut settings = Settings::default();
        settings.core.workspace.mode = bad.to_owned();
        assert_eq!(
            settings.core.workspace.mode_enum(),
            tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
            "{bad}"
        );
        assert!(
            matches!(validate_settings(&settings), Err(SettingsError::Invalid(_))),
            "{bad}"
        );
    }
    // Non-string JSON shapes fail deserialization (malformed, never reach
    // validation); unknown sibling fields keep existing behavior.
    for raw in ["null", "0", "true"] {
        let doc = serde_json::json!({
            "v": SETTINGS_SCHEMA_VERSION,
            "revision": 1,
            "core": {"workspace": {"mode": serde_json::from_str::<serde_json::Value>(raw).expect("shape"), "extra_kept": 1}},
            "bindings": {}
        });
        assert!(
            serde_json::from_value::<Settings>(doc).is_err(),
            "non-string shape refuses: {raw}"
        );
    }
}

#[test]
fn workspace_mode_presets_leave_core_field_unchanged() {
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.workspace.mode = WORKSPACE_MODE_SHARED.to_owned();
        apply_preset(&mut settings, preset);
        assert_eq!(settings.core.workspace.mode, WORKSPACE_MODE_SHARED);
        assert!(validate_settings(&settings).is_ok());
    }
}

#[test]
fn born_maximized_options_carry_functional_labels_and_wm_tips() {
    assert_eq!(
        born_maximized_options(),
        [
            (
                BORN_MAXIMIZED_TILE,
                "Tile on launch",
                "bspwm, i3, xmonad, sway, qtile"
            ),
            (
                BORN_MAXIMIZED_OVERLAY,
                "Keep maximized overlay",
                "COSMIC, Hyprland"
            ),
        ]
    );
}

#[test]
fn born_maximized_tokens_map_exactly_with_tile_default() {
    // Exact tokens only: no aliases, no case folding, no empty token.
    let mut settings = Settings::default();
    assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_TILE);
    assert!(!settings.core.born_maximized_keep_overlay());
    settings.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
    assert!(settings.core.born_maximized_keep_overlay());
    assert!(validate_settings(&settings).is_ok());
    for bad in [
        "",
        "tile",
        "keep-overlay",
        "TILE-ON-LAUNCH",
        "overlay",
        "group-with-neighbor",
    ] {
        let mut settings = Settings::default();
        settings.core.born_maximized = bad.to_owned();
        assert!(!settings.core.born_maximized_keep_overlay(), "{bad}");
        assert!(
            matches!(validate_settings(&settings), Err(SettingsError::Invalid(_))),
            "{bad}"
        );
    }
}

#[test]
fn born_maximized_shared_fixture_conformance() {
    use tiler_core::born_maximized::BornMaximized;
    // Windows wire constants pin the shared tokens and default.
    assert_eq!(
        BornMaximized::TileOnLaunch.as_wire_str(),
        BORN_MAXIMIZED_TILE
    );
    assert_eq!(
        BornMaximized::KeepMaximizedOverlay.as_wire_str(),
        BORN_MAXIMIZED_OVERLAY
    );
    assert_eq!(BornMaximized::default(), BornMaximized::TileOnLaunch);
    assert_eq!(Settings::default().core.born_maximized, BORN_MAXIMIZED_TILE);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../test-fixtures/born-maximized.json"
    ))
    .expect("shared born-maximized fixture parses");
    let cases = fixture
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .expect("fixture carries a case table");
    assert!(!cases.is_empty(), "fixture case table is non-empty");
    for case in cases {
        let name = case
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unnamed>");
        // A missing `raw` key pins the absent setting; any non-string
        // JSON shape (null, number, boolean) normalizes as missing.
        let raw = case.get("raw").and_then(serde_json::Value::as_str);
        let mode = BornMaximized::normalize_wire(raw);
        let normalized = case
            .get("normalized")
            .and_then(serde_json::Value::as_str)
            .expect("each case pins its normalized token");
        assert_eq!(mode.as_wire_str(), normalized, "{name}");
        let should_clear = case
            .get("should_clear")
            .and_then(serde_json::Value::as_bool)
            .expect("each case pins its clear-vs-keep outcome");
        assert_eq!(mode.should_clear_at_admission(), should_clear, "{name}");
        // Windows adapter boundary agrees on the normalized value.
        let mut settings = Settings::default();
        settings.core.born_maximized = mode.as_wire_str().to_owned();
        assert_eq!(settings.core.born_maximized_mode(), mode, "{name}");
        assert_eq!(
            !settings.core.born_maximized_keep_overlay(),
            should_clear,
            "{name}"
        );
        assert_eq!(
            crate::tiling::born_maximized_keep_overlay(mode.as_wire_str()),
            !should_clear,
            "{name}"
        );
        assert!(validate_settings(&settings).is_ok(), "{name}");
        // Strict saved-value refusal: an invalid raw string never becomes
        // live (last-good kept); fallback normalization is for hand-built
        // values only, never for accepting a saved file.
        if let Some(raw_str) = raw
            && BornMaximized::parse_wire(raw_str).is_none()
        {
            let mut invalid = Settings::default();
            invalid.core.born_maximized = raw_str.to_owned();
            assert!(!invalid.core.born_maximized_keep_overlay(), "{name}");
            assert!(
                matches!(validate_settings(&invalid), Err(SettingsError::Invalid(_))),
                "{name}"
            );
            assert!(
                !crate::tiling::born_maximized_keep_overlay(raw_str),
                "{name}"
            );
        }
    }
    // Strict saved-file serde boundary for the same fixture shapes:
    // the absent key deserializes to the tile default and validates;
    // invalid strings deserialize but refuse in validation (last-good
    // kept, never normalized into live); non-string shapes (null/0/true)
    // fail deserialization (malformed, never reach validation).
    assert!(
        cases.iter().any(|case| case.get("raw").is_none()),
        "fixture pins the absent key"
    );
    let absent: Settings = serde_json::from_value(serde_json::json!({
        "v": SETTINGS_SCHEMA_VERSION,
        "revision": 1,
        "core": {},
        "bindings": {}
    }))
    .expect("absent key deserializes");
    assert_eq!(absent.core.born_maximized, BORN_MAXIMIZED_TILE);
    assert!(validate_settings(&absent).is_ok());
    let mut invalid_strings = 0;
    let mut non_string_shapes = 0;
    for case in cases {
        let name = case
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unnamed>");
        let Some(raw) = case.get("raw") else {
            continue;
        };
        if let Some(raw_str) = raw.as_str() {
            if BornMaximized::parse_wire(raw_str).is_some() {
                continue;
            }
            invalid_strings += 1;
            let parsed: Settings = serde_json::from_value(serde_json::json!({
                "v": SETTINGS_SCHEMA_VERSION,
                "revision": 1,
                "core": {"born_maximized": raw_str},
                "bindings": {}
            }))
            .expect("invalid string still deserializes");
            assert!(
                matches!(validate_settings(&parsed), Err(SettingsError::Invalid(_))),
                "{name}"
            );
        } else {
            non_string_shapes += 1;
            let doc = serde_json::json!({
                "v": SETTINGS_SCHEMA_VERSION,
                "revision": 1,
                "core": {"born_maximized": raw},
                "bindings": {}
            });
            assert!(
                serde_json::from_value::<Settings>(doc).is_err(),
                "non-string shape refuses: {name}"
            );
        }
    }
    assert_eq!(invalid_strings, 2, "fixture pins bogus/empty strings");
    assert_eq!(non_string_shapes, 3, "fixture pins null/0/true shapes");
}

#[test]
fn born_maximized_presets_leave_core_field_unchanged() {
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        settings.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
        apply_preset(&mut settings, preset);
        assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
        assert!(validate_settings(&settings).is_ok());
    }
}

#[test]
fn chord_grammar_accepts_win_family_and_renders_canonically() {
    let parsed = parse_chord("win+shift+L").expect("parse");
    assert!(parsed.shift && !parsed.alt && !parsed.ctrl);
    assert_eq!(parsed.vk, 0x4C);
    assert_eq!(render_chord(&parsed), "Win+Shift+L");
    assert_eq!(
        render_chord(&parse_chord("Shift+Win+h").expect("order")),
        "Win+Shift+H"
    );
    assert_eq!(
        render_chord(&parse_chord("WIN+F11").expect("fkey")),
        "Win+F11"
    );
    assert_eq!(
        render_chord(&parse_chord("win+left").expect("arrow")),
        "Win+Left"
    );
    assert!(is_lock_chord(&parse_chord("Win+L").expect("lock")));
    assert!(!is_lock_chord(&parse_chord("Win+Shift+L").expect("move")));
    assert!(!is_lock_chord(&parse_chord("Win+Alt+L").expect("alt")));
}

#[test]
fn chord_grammar_refuses_malformed() {
    for bad in [
        "H",
        "Win",
        "Win+",
        "+Win+H",
        "Win+H+H",
        "Win+Win+H",
        "Ctrl+H",
        "Win+Foo",
        "Win+Shift",
        "Win++H",
        "",
        "Win+Escape",
    ] {
        assert!(parse_chord(bad).is_err(), "{bad}");
    }
    // Tab is a known key for the item 1 toggle.
    assert_eq!(
        render_chord(&parse_chord("Win+Ctrl+Tab").expect("tab")),
        "Win+Ctrl+Tab"
    );
}

#[test]
fn catalog_covers_kde_actions_without_collisions() {
    let catalog = binding_catalog();
    // 16 focus/move (letter plus separate arrow rows) + 8 resize + 6
    // toggles + 2 launchers + 20 workspace + 9 item 1 history + 10
    // numbered stay + 8 relative follow + 8 relative stay (item 2) + 8
    // output follow + 4 output stay (item 5) + 4 migration (item 14) =
    // 103.
    assert_eq!(catalog.len(), 103);
    let mut ids: Vec<&str> = catalog.iter().map(|def| def.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 103);
    // Every default parses and every row (resize included) intercepts.
    for def in &catalog {
        for default in def.defaults {
            parse_chord(default).expect("catalog chord parses");
        }
        assert!(def.implemented, "{}", def.id);
    }
    // Resize rows carry both letter and arrow defaults with the honest
    // unknown-ownership note, and ride the Alt arms through
    // mode-carrying actions.
    let resize_out = catalog
        .iter()
        .find(|def| def.id == "resize-out-left")
        .expect("row");
    assert_eq!(resize_out.defaults, &["Win+Alt+H", "Win+Alt+Left"]);
    assert!(
        resize_out
            .conflict
            .is_some_and(|c| c.contains("unverified in repository") && c.contains("unproven live"))
    );
    assert!(!binding_wants_shift(resize_out));
    assert!(!binding_wants_ctrl(resize_out));
    assert!(binding_wants_alt(resize_out));
    assert_eq!(
        binding_action(resize_out),
        crate::snapkey::ChordAction::ResizeOut
    );
    let resize_in = catalog
        .iter()
        .find(|def| def.id == "resize-in-left")
        .expect("row");
    assert_eq!(
        resize_in.defaults,
        &["Win+Alt+Shift+H", "Win+Alt+Shift+Left"]
    );
    assert!(binding_wants_shift(resize_in));
    assert!(binding_wants_alt(resize_in));
    assert_eq!(
        binding_action(resize_in),
        crate::snapkey::ChordAction::ResizeIn
    );
    assert_eq!(binding_arm_text(resize_out), "Win+Alt");
    assert_eq!(binding_arm_text(resize_in), "Win+Shift+Alt");
    // Focus-right keeps the lock-gated default with a truthful conflict.
    let focus_right = catalog
        .iter()
        .find(|def| def.id == "focus-right")
        .expect("row");
    assert!(focus_right.conflict.is_some());
    // Letter and arrow rows are separate bindings with distinct chords.
    let focus_left_arrow = catalog
        .iter()
        .find(|def| def.id == "focus-left-arrow")
        .expect("row");
    assert_eq!(focus_left_arrow.defaults, &["Win+Left"]);
    assert!(focus_left_arrow.conflict.is_some());
    // Item 1 history rows: toggle plus eight relative steps, all
    // implemented with honest ownership text.
    let previous = catalog
        .iter()
        .find(|def| def.id == "workspace-previous")
        .expect("row");
    assert_eq!(previous.defaults, &["Win+Ctrl+Tab"]);
    assert!(
        previous
            .conflict
            .is_some_and(|c| c.contains("unverified in repository"))
    );
    let prev_arrow = catalog
        .iter()
        .find(|def| def.id == "workspace-prev-left-arrow")
        .expect("row");
    assert_eq!(prev_arrow.defaults, &["Win+Ctrl+Left"]);
    assert!(
        prev_arrow
            .conflict
            .is_some_and(|c| c.contains("virtual desktop"))
    );
    assert!(binding_wants_ctrl(previous));
    assert!(!binding_wants_shift(previous));
    assert!(!binding_wants_alt(previous));
}

#[test]
fn validation_rejects_bad_core_values() {
    let mut settings = Settings::default();
    settings.core.inner_gap = 65;
    assert!(validate_settings(&settings).is_err());
    settings.core.inner_gap = 8;
    settings.core.outer_gap = -1;
    assert!(validate_settings(&settings).is_err());
    settings.core.outer_gap = 8;
    settings.core.border.width = 33.0;
    assert!(validate_settings(&settings).is_err());
    settings.core.border.width = 3.0;
    settings.core.border.color = "#ffff".to_owned();
    assert!(validate_settings(&settings).is_err());
    settings.core.border.color = "#2a82da".to_owned();
    settings.core.underlay.color = "#808080".to_owned();
    assert!(validate_settings(&settings).is_err());
    settings.core.underlay.color = "#40808080".to_owned();
    settings.core.underlay.extension = -2.0;
    assert!(validate_settings(&settings).is_err());
    settings.core.underlay.extension = -1.0;
    assert!(validate_settings(&settings).is_ok());
    // Explicitly saved colors survive the default change: the superseded
    // default and the dogfood value both validate with schema still v1.
    for explicit in ["#662a82da", "#40808080", "#50909090"] {
        settings.core.underlay.color = explicit.to_owned();
        assert!(validate_settings(&settings).is_ok(), "{explicit}");
        let options = tile_options_from_settings(&settings);
        assert_eq!(
            crate::group_underlay::render_color_argb((
                options.underlay.style.color,
                options.underlay.style.alpha
            )),
            explicit
        );
    }
    assert_eq!(SETTINGS_SCHEMA_VERSION, 1);
}

#[test]
fn omitted_underlay_color_defaults_to_candidate_c() {
    // Omitted fields deserialize to the new default; an explicit old
    // default round-trips unchanged through serde.
    let settings: Settings = serde_json::from_str(r#"{"v":1,"core":{}}"#).expect("minimal parses");
    assert_eq!(settings.core.underlay.color, "#662a82da");
    assert!(validate_settings(&settings).is_ok());
    let settings: Settings =
        serde_json::from_str(r##"{"v":1,"core":{"underlay":{"color":"#40808080"}}}"##)
            .expect("explicit old parses");
    assert_eq!(settings.core.underlay.color, "#40808080");
    assert!(validate_settings(&settings).is_ok());
    let settings: Settings =
        serde_json::from_str(r##"{"v":1,"core":{"underlay":{"color":"#50909090"}}}"##)
            .expect("explicit dogfood parses");
    assert_eq!(settings.core.underlay.color, "#50909090");
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn validation_rejects_unknown_incoherent_and_unsafe_bindings() {
    let mut settings = Settings::default();
    settings.bindings.insert(
        "nope".to_owned(),
        BindingSetting {
            state: BindingState::Keep,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_err());
    settings.bindings.clear();
    // Chord without rebind refuses.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Keep,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    // Rebind without chord refuses.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_err());
    // Lock-chord target refuses (no insecure bypass via remap).
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+L".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    // Matching Shift polarity validates: the rebound chord alone routes,
    // so focus (unshifted arm) needs Win without Shift.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    // ...but the shifted arm refuses for a focus binding: it would arm an
    // unintended move instead of the rebound focus.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    // ...and the shifted arm validates for a move binding on the same key:
    // one physical key may serve two arms (like float/sticky share G),
    // just never the same chord twice.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    settings.bindings.insert(
        "move-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+U".to_owned()),
        },
    );
    // ...but both route into one canonical key (H) sharing one hold
    // slot, so the second refuses: a rebound action takes exactly one
    // physical key (float/sticky excepted below).
    assert!(validate_settings(&settings).is_err());
    settings.bindings.remove("move-left");
    assert!(validate_settings(&settings).is_ok());
    // ...but two bindings may never claim the same rebound chord.
    settings.bindings.insert(
        "focus-down".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    settings.bindings.remove("focus-down");
    assert!(validate_settings(&settings).is_ok());
    // Float/sticky may share the G key across their separate arm slots:
    // the same physical key serves both arms with matching polarity.
    settings.bindings.clear();
    settings.bindings.insert(
        "toggle-float".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    settings.bindings.insert(
        "toggle-sticky".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    settings.bindings.clear();
    // Keep-equivalent rebind (custom equals the row's own default):
    // valid, routes natively, claims nothing.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+H".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    assert!(build_remap(&settings).is_empty());
    assert!(build_disabled(&settings).is_empty());
    settings.bindings.clear();
    // Resize rebinds ride the dedicated Alt arms: outwards needs Win+Alt,
    // inwards needs Win+Shift+Alt, each carrying its mode. The wrong
    // Shift polarity, a missing Alt, or an extra Ctrl refuses.
    settings.bindings.insert(
        "resize-out-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(
        remap[0],
        ChordRemap {
            from_vk: 0x55,
            from_shift: false,
            from_ctrl: false,
            from_alt: true,
            action: crate::snapkey::ChordAction::ResizeOut,
            to_vk: 0x48,
        }
    );
    // The rebound-away defaults pass through via suppression (both the
    // letter and arrow defaults of the rebound row).
    let disabled = build_disabled(&settings);
    assert!(disabled.contains(&ChordDisable {
        vk: 0x48,
        shift: false,
        ctrl: false,
        alt: true,
    }));
    assert!(disabled.contains(&ChordDisable {
        vk: VK_LEFT,
        shift: false,
        ctrl: false,
        alt: true,
    }));
    settings.bindings.clear();
    for bad in ["Win+U", "Win+Shift+U", "Win+Ctrl+Alt+U", "Win+Alt+Shift+U"] {
        settings.bindings.clear();
        settings.bindings.insert(
            "resize-out-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(bad.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{bad}");
    }
    settings.bindings.clear();
    settings.bindings.insert(
        "resize-in-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+Shift+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    settings.bindings.clear();
    // Unshifted Win+L can never be a rebind target, resize included.
    settings.bindings.insert(
        "resize-out-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+L".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    settings.bindings.remove("resize-out-left");
    assert!(validate_settings(&settings).is_ok());
    // Alt rebind refuses on non-Alt arms.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
}

#[test]
fn validation_rejects_duplicate_active_chords() {
    let mut settings = Settings::default();
    // Win+J is focus-down: rebinding focus-left onto it collides.
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+J".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    // A disabled binding frees its chord: disabling focus-down then
    // rebinding focus-left onto Win+J validates. Rows are per-chord, so
    // the Win+Down arrow row stays active on its own chord.
    settings.bindings.insert(
        "focus-down".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn presets_restore_defaults_and_preserve_win_l_opt_in() {
    let mut settings = Settings::default();
    settings.core.keyboard.allow_win_l = true;
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    settings.bindings.insert(
        "toggle-float".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    let changed = apply_preset(&mut settings, Preset::Authentic);
    assert!(changed.contains(&"focus-left"));
    assert!(changed.contains(&"toggle-float"));
    assert!(settings.bindings.is_empty());
    assert!(settings.core.keyboard.allow_win_l);
    assert!(validate_settings(&settings).is_ok());
    // Compatible disables 39 OS-conflicting rows and rebinds the two Xbox
    // chords to Win+Ctrl+G / Win+Ctrl+F11, preserving the
    // opt-in. From empty state the change is exactly the disable list plus
    // the two rebinds.
    let changed = apply_preset(&mut settings, Preset::Compatible);
    assert_eq!(
        changed.len(),
        compatible_disabled_ids().len() + compatible_rebinds().len()
    );
    for id in compatible_disabled_ids() {
        assert!(changed.contains(&id), "{id}");
    }
    for (id, _) in compatible_rebinds() {
        assert!(changed.contains(&id), "{id}");
    }
    assert!(changed.contains(&"focus-left"));
    assert!(changed.contains(&"focus-left-arrow"));
    assert!(changed.contains(&"toggle-float"));
    assert!(changed.contains(&"toggle-fullscreen"));
    assert!(changed.contains(&"workspace-select-1"));
    assert!(changed.contains(&"workspace-prev-left-arrow"));
    assert!(changed.contains(&"workspace-next-right-arrow"));
    assert!(!changed.contains(&"workspace-previous"));
    assert!(!changed.contains(&"workspace-prev-h"));
    assert_eq!(
        settings.bindings.get("toggle-float").map(|b| &b.state),
        Some(&BindingState::Rebind)
    );
    assert_eq!(
        settings
            .bindings
            .get("toggle-float")
            .and_then(|b| b.chord.clone()),
        Some("Win+Ctrl+G".to_owned())
    );
    assert_eq!(
        settings
            .bindings
            .get("toggle-fullscreen")
            .and_then(|b| b.chord.clone()),
        Some("Win+Ctrl+F11".to_owned())
    );
    assert!(!settings.bindings.contains_key("move-left"));
    assert!(!settings.bindings.contains_key("toggle-sticky"));
    assert!(!settings.bindings.contains_key("workspace-previous"));
    let effective = effective_bindings(&settings);
    assert_eq!(effective.len(), 103);
    for row in &effective {
        if compatible_disabled_ids().contains(&row.id) {
            assert!(!row.active, "{}", row.id);
            assert!(!row.effective, "{}", row.id);
        } else if row.id == "move-left"
            || row.id == "move-down"
            || row.id == "move-up"
            || row.id == "move-right"
            || row.id == "toggle-sticky"
            || row.id == "toggle-float"
            || row.id == "toggle-fullscreen"
        {
            assert!(row.active && row.effective, "{}", row.id);
        }
    }
    for (id, chord) in [
        ("toggle-float", "Win+Ctrl+G"),
        ("toggle-fullscreen", "Win+Ctrl+F11"),
    ] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert_eq!(row.chords, vec![chord.to_owned()], "{id}");
    }
    assert!(settings.core.keyboard.allow_win_l);
    assert!(validate_settings(&settings).is_ok());
    // Deterministic reset: a stale rebound (even at an OS-owned chord)
    // does not survive the preset.
    settings.bindings.insert(
        "move-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+Q".to_owned()),
        },
    );
    let changed = apply_preset(&mut settings, Preset::Compatible);
    assert!(!settings.bindings.contains_key("move-left"));
    assert_eq!(
        settings.bindings.get("focus-left").map(|b| &b.state),
        Some(&BindingState::Disabled)
    );
    assert!(changed.contains(&"move-left"));
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn chord_conflict_names_the_physical_os_owner() {
    assert!(
        chord_conflict("Win+H")
            .is_some_and(|text| text.contains("Voice") && text.contains("unproven"))
    );
    // Win+Shift+H has no documented owner: honest unverified note, never
    // a definitive clean claim.
    assert!(
        chord_conflict("Win+Shift+H").is_some_and(|text| text.contains("No documented conflict"))
    );
    // Win+U opens Accessibility settings (official list, Windows 11 tab;
    // Ease of Access Center on Windows 10): rebinding onto it stays
    // allowed (conflict text is advisory; only lock-chord targets refuse
    // in validation), but the UI must name the owner.
    assert!(chord_conflict("Win+U").is_some_and(|text| text.contains("Accessibility")));
    assert!(chord_conflict("Win+E").is_some_and(|text| text.contains("File Explorer")));
    assert!(chord_conflict("Win+Z").is_some_and(|text| text.contains("snap layouts")));
    assert!(chord_conflict("Win+Shift+S").is_some_and(|text| text.contains("Snipping")));
    assert!(chord_conflict("Win+G").is_some_and(|text| text.contains("cannot fully contain")));
    assert!(
        chord_conflict("Win+Shift+G").is_some_and(|text| text.contains("No documented conflict"))
    );
    assert!(chord_conflict("Win+L").is_some_and(|text| text.contains("opt-in")));
    assert!(chord_conflict("Win+1").is_some_and(|text| text.contains("taskbar")));
    assert!(chord_conflict("bogus").is_none());
    // Rebind rows report the rebound chord's conflict, not the stale
    // original: focus-left onto Win+U names Accessibility despite H's
    // voice-dictation owner, and the rebind itself still validates.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "focus-left")
        .expect("row");
    assert_eq!(row.chords, vec!["Win+U".to_owned()]);
    assert!(
        row.conflict
            .is_some_and(|text| text.contains("Accessibility"))
    );
}

#[test]
fn remap_routes_win_family_rebinds_only() {
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    // One full-modifier entry: the unshifted rebound routes to focus.
    // The shifted chord on the same key stays native (no unintended move).
    let remap = build_remap(&settings);
    assert_eq!(
        remap,
        vec![ChordRemap {
            from_vk: 0x55,
            from_shift: false,
            from_ctrl: false,
            from_alt: false,
            action: crate::snapkey::ChordAction::Directional,
            to_vk: 0x48
        },]
    );
    // The rebound-away default passes through via the suppression table.
    let disabled = build_disabled(&settings);
    assert_eq!(
        disabled,
        vec![ChordDisable {
            vk: 0x48,
            shift: false,
            ctrl: false,
            alt: false
        }]
    );
    // Rebinding onto the canonical key is a valid no-op (no entry).
    let mut same = Settings::default();
    same.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+H".to_owned()),
        },
    );
    assert!(validate_settings(&same).is_ok());
    assert!(build_remap(&same).is_empty());
}

#[test]
fn disabled_rows_suppress_their_chords_and_rebinds_free_theirs() {
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-float".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    settings.bindings.insert(
        "focus-left-arrow".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_ok());
    // Win+G unshifted and Win+Left unshifted pass through; the Win+H
    // letter row is untouched (per-chord rows).
    assert_eq!(
        build_disabled(&settings),
        vec![
            ChordDisable {
                vk: VK_LEFT,
                shift: false,
                ctrl: false,
                alt: false
            },
            ChordDisable {
                vk: 0x47,
                shift: false,
                ctrl: false,
                alt: false
            },
        ]
    );
    assert!(build_remap(&settings).is_empty());
}

#[test]
fn version_gate_refuses_unknown_schema() {
    let settings = Settings {
        v: 2,
        ..Settings::default()
    };
    assert_eq!(
        validate_settings(&settings),
        Err(SettingsError::UnsupportedVersion)
    );
}

#[test]
fn history_rebind_needs_ctrl_and_shares_vk_across_actions() {
    // Item 1 history arms ride Win+Ctrl: unshifted Win-only refuses,
    // shifted Ctrl refuses, Alt refuses, and the Ctrl arm validates.
    for bad in ["Win+U", "Win+Shift+U", "Win+Alt+U", "Win+Ctrl+Shift+U"] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "workspace-prev-h".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(bad.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{bad}");
    }
    let mut settings = Settings::default();
    settings.bindings.insert(
        "workspace-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    // History shares H with the directional focus arm across separate
    // slots: keeping Win+H focus while rebound history rides Win+Ctrl+U
    // validates (distinct explicit actions).
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(remap[0].from_vk, 0x55);
    assert!(remap[0].from_ctrl);
    assert_eq!(remap[0].action, crate::snapkey::ChordAction::WorkspacePrev);
    // Two history prev rows share no canonical slot (distinct VKs), but
    // two rebinds onto the same history action+VK refuse.
    settings.bindings.insert(
        "workspace-prev-k".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
}
