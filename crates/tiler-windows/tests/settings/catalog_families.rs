#![forbid(unsafe_code)]
use super::*;

// Keyboard resize catalog/settings: 8 logical rows (letter plus arrow alias
// each, 16 physical chords) on the dedicated Win+Alt / Win+Shift+Alt arms
// with mode-carrying actions. Both presets keep the defaults (Windows
// Alt-chord ownership unverified in the repository; containment unproven
// live); Compatible still disables known conflicts only.

#[test]
fn catalog_exposes_resize_rows_with_exact_modifiers() {
    use tiler_windows::settings::{
        RESIZE_CONFLICT, binding_action, binding_arm_text, binding_canonical_vk, binding_catalog,
        binding_modifiers_ok, binding_wants_alt, binding_wants_ctrl, binding_wants_shift,
        chord_conflict, effective_bindings, parse_chord,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    assert_eq!(catalog.len(), 103);
    let resize: Vec<_> = catalog
        .iter()
        .filter(|def| {
            matches!(
                def.family,
                tiler_windows::settings::BindingFamily::Resize { .. }
            )
        })
        .collect();
    assert_eq!(resize.len(), 8);
    for def in &resize {
        assert_eq!(
            def.defaults.len(),
            2,
            "{} carries letter plus arrow",
            def.id
        );
        assert!(def.implemented, "{}", def.id);
        for default in def.defaults {
            parse_chord(default).expect("catalog chord parses");
        }
        let conflict = def.conflict.expect("honest ownership text");
        assert_eq!(conflict, RESIZE_CONFLICT);
        assert!(
            conflict.contains("unverified in repository") && conflict.contains("unproven live"),
            "{}: {conflict}",
            def.id
        );
        // Canonical VK is the letter (first default), never the arrow alias:
        // the rebind routes into the letter hold slot through its explicit
        // action.
        let tiler_windows::settings::BindingFamily::Resize { direction, .. } = &def.family else {
            panic!("{} is a resize row", def.id);
        };
        let letter_vk = match *direction {
            "left" => 0x48,  // H
            "down" => 0x4A,  // J
            "up" => 0x4B,    // K
            "right" => 0x4C, // L
            other => panic!("{} has unknown direction {other}", def.id),
        };
        assert_eq!(
            binding_canonical_vk(def),
            Some(letter_vk),
            "{} canonical is the letter VK",
            def.id
        );
    }
    let out = catalog
        .iter()
        .find(|def| def.id == "resize-out-left")
        .expect("row");
    assert_eq!(out.defaults, &["Win+Alt+H", "Win+Alt+Left"]);
    assert!(!binding_wants_shift(out) && !binding_wants_ctrl(out) && binding_wants_alt(out));
    assert_eq!(binding_action(out), ChordAction::ResizeOut);
    assert_eq!(binding_arm_text(out), "Win+Alt");
    let inn = catalog
        .iter()
        .find(|def| def.id == "resize-in-right")
        .expect("row");
    assert_eq!(inn.defaults, &["Win+Alt+Shift+L", "Win+Alt+Shift+Right"]);
    assert!(binding_wants_shift(inn) && binding_wants_alt(inn) && !binding_wants_ctrl(inn));
    assert_eq!(binding_action(inn), ChordAction::ResizeIn);
    assert_eq!(binding_arm_text(inn), "Win+Shift+Alt");
    // Exact arms: outwards matches Win+Alt only, inwards Win+Shift+Alt only.
    let parsed = parse_chord("Win+Alt+H").expect("parse");
    assert!(binding_modifiers_ok(
        out,
        parsed.shift,
        parsed.ctrl,
        parsed.alt
    ));
    for bad in ["Win+H", "Win+Shift+H", "Win+Ctrl+Alt+H", "Win+Alt+Shift+H"] {
        let parsed = parse_chord(bad).expect("parse");
        assert!(
            !binding_modifiers_ok(out, parsed.shift, parsed.ctrl, parsed.alt),
            "{bad}"
        );
    }
    // Rebound chords report the same honest unknown-ownership text, never a
    // stock-holder or conflict-free claim.
    assert_eq!(chord_conflict("Win+Alt+U"), Some(RESIZE_CONFLICT));
    assert_eq!(chord_conflict("Win+Alt+Shift+U"), Some(RESIZE_CONFLICT));
    // Effective defaults are live under both presets.
    let effective = effective_bindings(&Settings::default());
    let row = effective
        .iter()
        .find(|row| row.id == "resize-out-left")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(
        row.chords,
        vec!["Win+Alt+H".to_owned(), "Win+Alt+Left".to_owned()]
    );
    assert_eq!(row.conflict, Some(RESIZE_CONFLICT));
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        tiler_windows::settings::apply_preset(&mut settings, preset);
        assert!(
            !settings.bindings.contains_key("resize-out-left"),
            "{preset:?} keeps resize"
        );
        assert!(
            !settings.bindings.contains_key("resize-in-right"),
            "{preset:?} keeps resize"
        );
        let effective = effective_bindings(&settings);
        let row = effective
            .iter()
            .find(|row| row.id == "resize-in-left")
            .expect("row");
        assert!(row.active && row.effective, "{preset:?}");
        assert!(tiler_windows::settings::validate_settings(&settings).is_ok());
    }
    // Compatible still disables a known conflict alongside the kept resize.
    let mut settings = Settings::default();
    tiler_windows::settings::apply_preset(&mut settings, Preset::Compatible);
    assert_eq!(
        settings.bindings.get("focus-left").map(|b| &b.state),
        Some(&BindingState::Disabled)
    );
}

// Keyboard resize Keep/Disable/Rebind: real transitions with full-modifier
// duplicate checks and the honest unknown-ownership rebound text.
#[test]
fn resize_keep_disable_rebind_with_duplicates() {
    use tiler_windows::settings::{
        RESIZE_CONFLICT, Settings, binding_catalog, build_disabled, build_remap,
        effective_bindings, validate_settings,
    };
    use tiler_windows::snapkey::{ChordAction, ChordDisable};
    // Disable suppresses both physical defaults (letter plus arrow).
    let mut settings = Settings::default();
    settings.bindings.insert(
        "resize-out-left".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let disabled = build_disabled(&settings);
    assert!(disabled.contains(&ChordDisable {
        vk: 0x48,
        shift: false,
        ctrl: false,
        alt: true
    }));
    assert!(disabled.contains(&ChordDisable {
        vk: 37,
        shift: false,
        ctrl: false,
        alt: true
    }));
    assert!(build_remap(&settings).is_empty());
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "resize-out-left")
        .expect("row");
    assert!(!row.active && !row.effective);
    assert_eq!(row.conflict, Some(RESIZE_CONFLICT));
    // Rebind routes the exact Alt arm into the mode-carrying action at the
    // letter canonical, suppresses both old defaults, and reports the
    // rebound chord's own unknown-ownership conflict.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "resize-out-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let remap = build_remap(&settings);
    assert_eq!(
        remap,
        vec![ChordRemap {
            from_vk: VK_U,
            from_shift: false,
            from_ctrl: false,
            from_alt: true,
            action: ChordAction::ResizeOut,
            to_vk: VK_H,
        }]
    );
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "resize-out-left")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+Alt+U".to_owned()]);
    assert_eq!(row.conflict, Some(RESIZE_CONFLICT));
    // Full-modifier duplicate: two rows onto one Alt chord refuse, while the
    // Shift and unshifted Alt variants on one key coexist (distinct chords).
    let mut settings = Settings::default();
    settings.bindings.insert(
        "resize-out-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+U".to_owned()),
        },
    );
    settings.bindings.insert(
        "resize-out-down".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    let mut settings = Settings::default();
    settings.bindings.insert(
        "resize-out-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+U".to_owned()),
        },
    );
    settings.bindings.insert(
        "resize-in-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Alt+Shift+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    // Wrong arms refuse (missing Alt, extra Ctrl, flipped Shift).
    for (id, bad) in [
        ("resize-out-left", "Win+U"),
        ("resize-out-left", "Win+Shift+U"),
        ("resize-out-left", "Win+Ctrl+Alt+U"),
        ("resize-in-left", "Win+Alt+U"),
        ("resize-in-left", "Win+U"),
    ] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(bad.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{id} {bad}");
    }
    // Unshifted Win+L can never be a target.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "resize-in-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+L".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    // Catalog conflict helper is reachable from the row (no dead helper).
    let catalog = binding_catalog();
    let def = catalog
        .iter()
        .find(|def| def.id == "resize-out-left")
        .expect("row");
    assert_eq!(def.conflict, Some(RESIZE_CONFLICT));
}

// Launcher catalog/settings: `launch-browser` on exact unshifted Win+B and
// `launch-terminal` on exact unshifted Win+T; Authentic keeps both (takeover
// overrides the notification-area/taskbar owners), Compatible disables both.
// Keep/Disable/Rebind ride the existing toggle-style plumbing with distinct
// launch actions at the B/T canonical slots.
#[test]
fn catalog_exposes_launcher_rows_with_exact_modifiers() {
    use tiler_windows::settings::{
        BindingSetting, BindingState, Preset, Settings, apply_preset, binding_action,
        binding_canonical_vk, binding_catalog, binding_modifiers_ok, binding_wants_alt,
        binding_wants_ctrl, binding_wants_shift, compatible_disabled_ids, effective_bindings,
        parse_chord, validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    for (id, chord, vk, action, owner) in [
        (
            "launch-browser",
            "Win+B",
            0x42,
            ChordAction::LaunchBrowser,
            "notification area focus",
        ),
        (
            "launch-terminal",
            "Win+T",
            0x54,
            ChordAction::LaunchTerminal,
            "taskbar cycle",
        ),
    ] {
        let def = catalog.iter().find(|def| def.id == id).expect("row");
        assert_eq!(def.defaults, &[chord]);
        assert!(def.implemented, "{id}");
        assert!(!binding_wants_shift(def), "{id}");
        assert!(!binding_wants_ctrl(def), "{id}");
        assert!(!binding_wants_alt(def), "{id}");
        assert_eq!(binding_action(def), action, "{id}");
        assert_eq!(binding_canonical_vk(def), Some(vk), "{id}");
        let conflict = def.conflict.expect("OS owner text");
        assert!(
            conflict.contains(owner) && conflict.contains("takeover"),
            "{id}: {conflict}"
        );
        // Exact modifiers: only the unshifted Win chord matches the arm.
        let parsed = parse_chord(chord).expect("parse");
        assert!(binding_modifiers_ok(
            def,
            parsed.shift,
            parsed.ctrl,
            parsed.alt
        ));
        let shifted = format!("Win+Shift+{}", &chord[4..]);
        for bad in [shifted.as_str(), "Win+Ctrl+B", "Win+Ctrl+T", "Win+Alt+B"] {
            let parsed = parse_chord(bad).expect("parse");
            assert!(
                !binding_modifiers_ok(def, parsed.shift, parsed.ctrl, parsed.alt),
                "{id} {bad} must not match"
            );
        }
    }
    // Authentic keeps both launchers live on their defaults.
    let effective = effective_bindings(&Settings::default());
    assert_eq!(effective.len(), 103);
    for (id, chord) in [("launch-browser", "Win+B"), ("launch-terminal", "Win+T")] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert!(row.active && row.effective, "{id}");
        assert_eq!(row.chords, vec![chord.to_owned()], "{id}");
    }
    // Compatible disables both.
    assert!(compatible_disabled_ids().contains(&"launch-browser"));
    assert!(compatible_disabled_ids().contains(&"launch-terminal"));
    let mut settings = Settings::default();
    apply_preset(&mut settings, Preset::Compatible);
    for id in ["launch-browser", "launch-terminal"] {
        assert_eq!(
            settings.bindings.get(id).map(|b| &b.state),
            Some(&BindingState::Disabled),
            "{id}"
        );
    }
    let effective = effective_bindings(&settings);
    for id in ["launch-browser", "launch-terminal"] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert!(!row.active && !row.effective, "{id}");
    }
    // Disable validates and suppresses the default chord through the shared
    // suppression table.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "launch-browser".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let disabled = tiler_windows::settings::build_disabled(&settings);
    assert!(disabled.contains(&tiler_windows::snapkey::ChordDisable {
        vk: 0x42,
        shift: false,
        ctrl: false,
        alt: false,
    }));
    // Rebind routes the unshifted arm into the launch action at the B
    // canonical slot and suppresses the old default.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "launch-browser".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let remap = tiler_windows::settings::build_remap(&settings);
    assert_eq!(
        remap,
        vec![tiler_windows::snapkey::ChordRemap {
            from_vk: 0x55,
            from_shift: false,
            from_ctrl: false,
            from_alt: false,
            action: ChordAction::LaunchBrowser,
            to_vk: 0x42,
        }]
    );
    let disabled = tiler_windows::settings::build_disabled(&settings);
    assert!(disabled.contains(&tiler_windows::snapkey::ChordDisable {
        vk: 0x42,
        shift: false,
        ctrl: false,
        alt: false,
    }));
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "launch-browser")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+U".to_owned()]);
    // Wrong-arm rebinds refuse.
    for (id, bad) in [
        ("launch-browser", "Win+Shift+U"),
        ("launch-browser", "Win+Ctrl+U"),
        ("launch-browser", "Win+Alt+U"),
        ("launch-terminal", "Win+Shift+U"),
        ("launch-terminal", "Win+Ctrl+U"),
    ] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(bad.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{id} {bad}");
    }
    // Duplicate active chords refuse: rebound browser onto the terminal
    // default, and both launchers onto one fresh chord.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "launch-browser".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+T".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    let mut settings = Settings::default();
    for id in ["launch-browser", "launch-terminal"] {
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
    }
    assert!(validate_settings(&settings).is_err());
}

// Item 21 workspace-tiling toggle: default Win+Y through the existing toggle
// arm, with the recorded Mixed Reality conflict. Authentic keeps it live;
// Compatible disables it; other unbound actions stay unbound.
#[test]
fn catalog_exposes_workspace_tiling_toggle_with_mixed_reality_conflict() {
    use tiler_windows::settings::{
        apply_preset, binding_action, binding_canonical_vk, binding_catalog, binding_modifiers_ok,
        compatible_disabled_ids, effective_bindings, parse_chord, validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    let def = catalog
        .iter()
        .find(|def| def.id == "toggle-workspace-tiling")
        .expect("row");
    assert_eq!(def.defaults, &["Win+Y"]);
    assert!(def.implemented);
    let conflict = def.conflict.expect("Mixed Reality conflict");
    assert!(
        conflict.contains("Mixed Reality") && conflict.contains("takeover"),
        "{conflict}"
    );
    assert_eq!(binding_action(def), ChordAction::WorkspaceTiling);
    assert_eq!(binding_canonical_vk(def), Some(0x59));
    // Exact modifiers: only the unshifted Win chord matches the arm.
    let parsed = parse_chord("Win+Y").expect("parse");
    assert!(binding_modifiers_ok(
        def,
        parsed.shift,
        parsed.ctrl,
        parsed.alt
    ));
    for bad in ["Win+Shift+Y", "Win+Ctrl+Y", "Win+Alt+Y"] {
        let parsed = parse_chord(bad).expect("parse");
        assert!(
            !binding_modifiers_ok(def, parsed.shift, parsed.ctrl, parsed.alt),
            "{bad} must not match"
        );
    }
    // Authentic keeps the toggle live on its default.
    let effective = effective_bindings(&Settings::default());
    let row = effective
        .iter()
        .find(|row| row.id == "toggle-workspace-tiling")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+Y".to_owned()]);
    // Compatible disables it (Authentic takes the Mixed Reality conflict).
    assert!(compatible_disabled_ids().contains(&"toggle-workspace-tiling"));
    let mut settings = Settings::default();
    apply_preset(&mut settings, Preset::Compatible);
    assert_eq!(
        settings
            .bindings
            .get("toggle-workspace-tiling")
            .map(|b| &b.state),
        Some(&BindingState::Disabled)
    );
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "toggle-workspace-tiling")
        .expect("row");
    assert!(!row.active && !row.effective, "toggle-workspace-tiling");
    // Disable validates and suppresses the default chord through the shared
    // suppression table.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-workspace-tiling".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let disabled = tiler_windows::settings::build_disabled(&settings);
    assert!(disabled.contains(&tiler_windows::snapkey::ChordDisable {
        vk: 0x59,
        shift: false,
        ctrl: false,
        alt: false,
    }));
    // Rebind routes the unshifted arm into the workspace-tiling action at the
    // Y canonical slot and suppresses the old default.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-workspace-tiling".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let remap = tiler_windows::settings::build_remap(&settings);
    assert_eq!(
        remap,
        vec![tiler_windows::snapkey::ChordRemap {
            from_vk: 0x55,
            from_shift: false,
            from_ctrl: false,
            from_alt: false,
            action: ChordAction::WorkspaceTiling,
            to_vk: 0x59,
        }]
    );
    let disabled = tiler_windows::settings::build_disabled(&settings);
    assert!(disabled.contains(&tiler_windows::snapkey::ChordDisable {
        vk: 0x59,
        shift: false,
        ctrl: false,
        alt: false,
    }));
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "toggle-workspace-tiling")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+U".to_owned()]);
    // Wrong-arm rebinds refuse.
    for bad in ["Win+Shift+U", "Win+Ctrl+U", "Win+Alt+U"] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "toggle-workspace-tiling".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(bad.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{bad}");
    }
    // Duplicate active chords refuse: rebound onto the orientation default.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-workspace-tiling".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+O".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
}

// Item 5 catalog/settings: 8 follow rows (letters plus separate arrow rows,
// Win+Ctrl+Alt defaults kept under both presets with the honest
// ownership-unknown note) plus 4 canonical stay rows (one per direction,
// bindable unbound). No Compatible disables and no invented stock holder.
#[test]
fn catalog_exposes_output_send_rows_with_exact_modifiers() {
    use tiler_windows::settings::{
        OUTPUT_SEND_CONFLICT, binding_action, binding_canonical_vk, binding_catalog,
        binding_modifiers_ok, binding_wants_alt, binding_wants_ctrl, binding_wants_shift,
        chord_conflict,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    assert_eq!(catalog.len(), 103);
    // Follow: eight bound rows on the Win+Ctrl+Alt arm through the follow
    // action, sharing the letter/arrow canonical slots.
    for (id, chord, action) in [
        (
            "send-output-left",
            "Win+Ctrl+Alt+H",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-left-arrow",
            "Win+Ctrl+Alt+Left",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-down",
            "Win+Ctrl+Alt+J",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-down-arrow",
            "Win+Ctrl+Alt+Down",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-up",
            "Win+Ctrl+Alt+K",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-up-arrow",
            "Win+Ctrl+Alt+Up",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-right",
            "Win+Ctrl+Alt+L",
            ChordAction::WorkspaceOutputFollow,
        ),
        (
            "send-output-right-arrow",
            "Win+Ctrl+Alt+Right",
            ChordAction::WorkspaceOutputFollow,
        ),
    ] {
        let def = catalog.iter().find(|def| def.id == id).expect("row");
        assert_eq!(def.defaults, &[chord]);
        assert!(def.implemented, "{id}");
        assert!(!binding_wants_shift(def), "{id} rides unshifted");
        assert!(binding_wants_ctrl(def) && binding_wants_alt(def), "{id}");
        assert_eq!(binding_action(def), action);
        assert_eq!(def.conflict, Some(OUTPUT_SEND_CONFLICT));
        // A shifted rebind of a follow row refuses (stay owns the Shift arm).
        assert!(!binding_modifiers_ok(def, true, true, true), "{id}");
    }
    // Stay: four unbound rows through the stay action, sharing the follow
    // letter slot by row identity.
    for (id, canon) in [
        ("send-output-left-stay", 0x48u32),
        ("send-output-down-stay", 0x4Au32),
        ("send-output-up-stay", 0x4Bu32),
        ("send-output-right-stay", 0x4Cu32),
    ] {
        let def = catalog.iter().find(|def| def.id == id).expect("row");
        assert!(def.defaults.is_empty(), "{id} unbound by default");
        assert!(def.implemented, "{id}");
        assert!(def.conflict.is_none(), "{id} unbound carries no conflict");
        assert_eq!(binding_action(def), ChordAction::WorkspaceOutputStay);
        assert_eq!(
            binding_canonical_vk(def),
            Some(canon),
            "{id} shares the letter slot"
        );
        // Stay accepts the unshifted arm (e.g. Win+Ctrl+Alt+F6) and the
        // shifted arm; anything else refuses.
        assert!(binding_modifiers_ok(def, false, true, true), "{id}");
        assert!(binding_modifiers_ok(def, true, true, true), "{id}");
        assert!(!binding_modifiers_ok(def, false, false, false), "{id}");
        assert!(!binding_modifiers_ok(def, false, true, false), "{id}");
    }
    // Conflict text is honestly unknown-ownership, never a stock holder or
    // conflict-free claim; the resize Alt arm is untouched.
    let conflict = chord_conflict("Win+Ctrl+Alt+H").expect("output conflict");
    assert!(
        conflict.contains("ownership unknown") && conflict.contains("unproven live"),
        "{conflict}"
    );
    assert!(!conflict.contains("virtual desktop"), "{conflict}");
    let resize = chord_conflict("Win+Alt+H").expect("resize conflict");
    assert_ne!(resize, OUTPUT_SEND_CONFLICT);
}

#[test]
fn output_send_presets_keep_both_with_no_new_disables() {
    use tiler_windows::settings::{
        apply_preset, binding_catalog, compatible_disabled_ids, effective_bindings,
    };
    for id in [
        "send-output-left",
        "send-output-left-arrow",
        "send-output-down",
        "send-output-down-arrow",
        "send-output-up",
        "send-output-up-arrow",
        "send-output-right",
        "send-output-right-arrow",
        "send-output-left-stay",
        "send-output-down-stay",
        "send-output-up-stay",
        "send-output-right-stay",
    ] {
        assert!(
            binding_catalog().iter().any(|def| def.id == id),
            "{id} in catalog"
        );
        assert!(
            !compatible_disabled_ids().contains(&id),
            "{id} keeps under Compatible"
        );
    }
    for preset in [Preset::Authentic, Preset::Compatible] {
        let mut settings = Settings::default();
        apply_preset(&mut settings, preset);
        assert!(
            !settings
                .bindings
                .keys()
                .any(|id| id.starts_with("send-output")),
            "no output overrides under {preset:?}"
        );
        let effective = effective_bindings(&settings);
        assert_eq!(effective.len(), 103);
        for row in effective
            .iter()
            .filter(|row| row.id.starts_with("send-output-"))
        {
            if row.id.ends_with("-stay") {
                assert!(row.active && !row.effective, "{}", row.id);
                assert!(row.chords.is_empty(), "{}", row.id);
            } else {
                assert!(row.active && row.effective, "{}", row.id);
                assert_eq!(row.chords.len(), 1, "{}", row.id);
            }
        }
    }
}

#[test]
fn output_send_rebind_remap_and_validation() {
    use tiler_windows::settings::{build_disabled, build_remap, validate_settings};
    use tiler_windows::snapkey::{ChordAction, ChordDisable, ChordRemap};
    // Follow rebind to an unused validated chord routes with the follow
    // action; the old default passes through.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-output-right".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Alt+F6".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    assert_eq!(
        build_remap(&settings),
        vec![ChordRemap {
            from_vk: 0x75,
            from_shift: false,
            from_ctrl: true,
            from_alt: true,
            action: ChordAction::WorkspaceOutputFollow,
            to_vk: 0x4C,
        }]
    );
    assert!(build_disabled(&settings).contains(&ChordDisable {
        vk: 0x4C,
        shift: false,
        ctrl: true,
        alt: true,
    }));
    // Stay rebind without Shift (the backlog journey chord) routes stay.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-output-right-stay".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Alt+F6".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    assert_eq!(
        build_remap(&settings),
        vec![ChordRemap {
            from_vk: 0x75,
            from_shift: false,
            from_ctrl: true,
            from_alt: true,
            action: ChordAction::WorkspaceOutputStay,
            to_vk: 0x4C,
        }]
    );
    // Stay rebind with Shift routes stay too.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-output-right-stay".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Alt+Shift+F6".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    // Wrong-arm rebinds refuse: shifted follow, unshifted-history-style stay,
    // lock-chord target.
    for (id, bad) in [
        ("send-output-right", "Win+Ctrl+Alt+Shift+F6"),
        ("send-output-right", "Win+Ctrl+F6"),
        ("send-output-right", "Win+Alt+F6"),
        ("send-output-right-stay", "Win+Shift+F6"),
        ("send-output-right-stay", "Win+Ctrl+F6"),
        ("send-output-right", "Win+L"),
    ] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(bad.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{id} {bad}");
    }
    // Duplicate active chords refuse: stay rebound onto the follow default.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-output-right-stay".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Alt+L".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
}
