#![forbid(unsafe_code)]
use super::*;

#[test]
fn compatible_preset_disables_os_conflicting_rows() {
    let mut settings = Settings::default();
    let changed = tiler_windows::settings::apply_preset(&mut settings, Preset::Compatible);
    // 39 disabled rows (8 focus + 4 move-arrow + 3 toggles including
    // workspace tiling + 2 launchers + 20 workspace digits + 2 history
    // arrows) plus the 2 Xbox Ctrl rebinds.
    assert_eq!(changed.len(), 41);
    assert_eq!(
        changed.len(),
        tiler_windows::settings::compatible_disabled_ids().len() + 2
    );
    assert!(changed.contains(&"toggle-float"));
    assert!(changed.contains(&"toggle-fullscreen"));
    assert!(changed.contains(&"toggle-orientation"));
    assert!(changed.contains(&"toggle-workspace-tiling"));
    assert!(changed.contains(&"launch-browser"));
    assert!(changed.contains(&"launch-terminal"));
    assert!(changed.contains(&"focus-left"));
    assert!(changed.contains(&"focus-left-arrow"));
    assert!(changed.contains(&"move-left-arrow"));
    assert!(changed.contains(&"workspace-select-1"));
    assert!(changed.contains(&"workspace-prev-left-arrow"));
    assert!(changed.contains(&"workspace-next-right-arrow"));
    let effective = tiler_windows::settings::effective_bindings(&settings);
    assert_eq!(effective.len(), 103);
    for row in &effective {
        if tiler_windows::settings::compatible_disabled_ids().contains(&row.id) {
            assert!(!row.active, "{}", row.id);
            assert!(!row.effective, "{}", row.id);
        }
    }
    // Conflict-free letter moves plus sticky stay active, plus the kept
    // history rows (Tab/letters/Up/Down) and the kept item 2 relative-send
    // follow rows (unknown ownership, no new Compatible disables), plus the
    // two Compatible Ctrl rebinds.
    for id in [
        "move-left",
        "move-down",
        "move-up",
        "move-right",
        "toggle-sticky",
        "toggle-float",
        "toggle-fullscreen",
        "workspace-previous",
        "workspace-prev-h",
        "workspace-next-j",
        "send-prev-h",
        "send-next-j",
    ] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert!(row.active && row.effective, "{id}");
    }
    for (id, chord) in [
        ("toggle-float", "Win+Ctrl+G"),
        ("toggle-fullscreen", "Win+Ctrl+F11"),
    ] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert_eq!(row.chords, vec![chord.to_owned()], "{id}");
    }
    // Item 2 stay rows stay unbound under both presets: active but with no
    // chord and not effective (bindable, distinctly not disabled).
    for id in ["stay-workspace-1", "send-stay-prev-h", "send-stay-next-j"] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert!(row.active, "{id}");
        assert!(row.chords.is_empty(), "{id}");
        assert!(!row.effective, "{id}");
    }
    // Conflict text discloses the incomplete containment honestly.
    let catalog = tiler_windows::settings::binding_catalog();
    let float = catalog
        .iter()
        .find(|def| def.id == "toggle-float")
        .expect("row");
    let conflict = float.conflict.expect("conflict text");
    assert!(conflict.contains("cannot fully contain"), "{conflict}");
    // Per-chord conflicts name their OS owner.
    let focus_left = catalog
        .iter()
        .find(|def| def.id == "focus-left")
        .expect("row");
    assert!(
        focus_left.conflict.is_some_and(|c| c.contains("Voice")),
        "{:?}",
        focus_left.conflict
    );
    // Win+L keeps its explicit opt-in gate: focus-right is disabled by the
    // preset, and the classifier still passes physical Win+L through without
    // opt-in.
    let focus_right = effective
        .iter()
        .find(|row| row.id == "focus-right")
        .expect("row");
    assert!(!focus_right.active);
    let mut machine = SnapClassify::new(takeover());
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    assert_eq!(
        SnapClassify::push(&mut machine, 0x4C, false, true, false),
        None,
        "unshifted Win+L passes through without opt-in"
    );
    // Authentic restores every binding while preserving the opt-in.
    settings.core.keyboard.allow_win_l = true;
    tiler_windows::settings::apply_preset(&mut settings, Preset::Authentic);
    assert!(settings.bindings.is_empty());
    assert!(settings.core.keyboard.allow_win_l);
    assert!(parse_chord("Win+L").is_ok());
}

#[test]
fn retained_update_gaps_preserves_topology() {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::session::{DomainKey, OutputDomain};

    fn rect() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }
    }
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let key = DomainKey {
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: rect(),
        gap: 8,
        adjacent: std::collections::BTreeMap::new(),
    };
    let rows = vec![
        (
            WindowId("w1".to_owned()),
            rect(),
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
        (
            WindowId("w2".to_owned()),
            rect(),
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
    ];
    let fingerprint = tiler_windows::tiling::fingerprint(
        &rows
            .iter()
            .map(|(t, r, _, _)| (t.0.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let correlation = CorrelationId::parse("seed").expect("correlation");
    let mut seed = tiler_windows::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        0,
        fingerprint,
        &domain,
        &key,
        8,
        &rows,
        Some(&WindowId("w1".to_owned())),
    );
    assert!(seed.command == CoreCommand::Reconcile);
    let reply = engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed converges, got {reply:?}"
    );
    let tree_before = format!(
        "{:?}",
        engine.session(&key).expect("session").snapshot().domains
    );
    // Plain reconcile with drifted gaps refuses instead of adopting.
    seed.command = CoreCommand::Reconcile;
    seed.domain.gap = 4;
    seed.outer_gap = 12;
    let reply = engine.handle(&seed);
    let CoreReply::Rejected { kind, .. } = reply else {
        panic!("gap drift must refuse, got {reply:?}");
    };
    assert_eq!(kind, "domain-mismatch");
    // The retained update-gaps path adopts without destroying topology.
    assert!(!tiler_windows::settings::retained_gaps_match(
        &engine, &key, 4, 12
    ));
    seed.command = CoreCommand::UpdateGaps;
    let reply = engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_)),
        "update-gaps adopts, got {reply:?}"
    );
    let tree_after = format!(
        "{:?}",
        engine.session(&key).expect("session").snapshot().domains
    );
    assert_eq!(tree_before, tree_after, "topology survives the gap update");
    assert!(tiler_windows::settings::retained_gaps_match(
        &engine, &key, 4, 12
    ));
    assert_eq!(engine.outer_gap(&key), Some(12));
}

// Item 2 catalog/settings: 26 new rows (10 numbered stay + 8 relative follow
// + 8 relative stay) for 83 total, plus the item 4 orientation toggle for 84,
// plus the two launcher rows for 86, plus the item 21 workspace-tiling toggle
// for 87, plus the 12 item 5 output rows (8 follow + 4 stay) for 99, plus the
// 4 item 14 migration rows for 103; follow
// defaults keep pending evidenced
// conflicts with honest unknown-ownership text (no new Compatible disables);
// stay and migration rows are bindable unbound with Keep meaning unbound.

#[test]
fn catalog_exposes_item2_rows_with_exact_modifiers() {
    use tiler_windows::settings::{
        binding_action, binding_canonical_vk, binding_catalog, binding_wants_ctrl,
        binding_wants_shift,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    assert_eq!(catalog.len(), 103);
    // Numbered stay: ten unbound rows on the shifted digit arm through the
    // stay action, sharing the follow digit canonical slot.
    for index in [1u8, 2, 9, 0] {
        let id = format!("stay-workspace-{index}");
        let def = catalog.iter().find(|def| def.id == id).expect("stay row");
        assert!(def.defaults.is_empty(), "{id} unbound by default");
        assert!(def.implemented, "{id}");
        assert!(binding_wants_shift(def), "{id}");
        assert!(!binding_wants_ctrl(def), "{id}");
        assert_eq!(binding_action(def), ChordAction::WorkspaceStayDigit);
        assert_eq!(
            binding_canonical_vk(def),
            Some(0x30 + u32::from(index)),
            "{id} shares the digit slot"
        );
    }
    // Relative follow: eight bound rows on the Win+Ctrl+Shift arm.
    for (id, chord, action) in [
        (
            "send-prev-h",
            "Win+Ctrl+Shift+H",
            ChordAction::WorkspaceSendPrev,
        ),
        (
            "send-prev-k",
            "Win+Ctrl+Shift+K",
            ChordAction::WorkspaceSendPrev,
        ),
        (
            "send-prev-left-arrow",
            "Win+Ctrl+Shift+Left",
            ChordAction::WorkspaceSendPrev,
        ),
        (
            "send-prev-up-arrow",
            "Win+Ctrl+Shift+Up",
            ChordAction::WorkspaceSendPrev,
        ),
        (
            "send-next-j",
            "Win+Ctrl+Shift+J",
            ChordAction::WorkspaceSendNext,
        ),
        (
            "send-next-l",
            "Win+Ctrl+Shift+L",
            ChordAction::WorkspaceSendNext,
        ),
        (
            "send-next-down-arrow",
            "Win+Ctrl+Shift+Down",
            ChordAction::WorkspaceSendNext,
        ),
        (
            "send-next-right-arrow",
            "Win+Ctrl+Shift+Right",
            ChordAction::WorkspaceSendNext,
        ),
    ] {
        let def = catalog.iter().find(|def| def.id == id).expect("row");
        assert_eq!(def.defaults, &[chord]);
        assert!(binding_wants_shift(def) && binding_wants_ctrl(def), "{id}");
        assert_eq!(binding_action(def), action);
        let conflict = def.conflict.expect("honest ownership text");
        assert!(
            conflict.contains("ownership unknown") && conflict.contains("unproven live"),
            "{id}: {conflict}"
        );
    }
    // Relative stay: eight unbound rows on the same arm with stay actions.
    for (id, action) in [
        ("send-stay-prev-h", ChordAction::WorkspaceSendStayPrev),
        ("send-stay-prev-k", ChordAction::WorkspaceSendStayPrev),
        (
            "send-stay-prev-left-arrow",
            ChordAction::WorkspaceSendStayPrev,
        ),
        (
            "send-stay-prev-up-arrow",
            ChordAction::WorkspaceSendStayPrev,
        ),
        ("send-stay-next-j", ChordAction::WorkspaceSendStayNext),
        ("send-stay-next-l", ChordAction::WorkspaceSendStayNext),
        (
            "send-stay-next-down-arrow",
            ChordAction::WorkspaceSendStayNext,
        ),
        (
            "send-stay-next-right-arrow",
            ChordAction::WorkspaceSendStayNext,
        ),
    ] {
        let def = catalog.iter().find(|def| def.id == id).expect("row");
        assert!(def.defaults.is_empty(), "{id} unbound by default");
        assert!(binding_wants_shift(def) && binding_wants_ctrl(def), "{id}");
        assert_eq!(binding_action(def), action);
        assert!(binding_canonical_vk(def).is_some(), "{id} has a slot");
    }
}

// Item 4 catalog/settings: one `toggle-orientation` row on exact unshifted
// Win+O; Authentic keeps it (OS orientation lock takeover), Compatible
// disables it. Keep/Disable/Rebind ride the existing toggle plumbing.
#[test]
fn catalog_exposes_orientation_row_with_exact_modifiers() {
    use tiler_windows::settings::{
        BindingSetting, BindingState, Preset, Settings, apply_preset, binding_action,
        binding_canonical_vk, binding_catalog, binding_modifiers_ok, binding_wants_ctrl,
        binding_wants_shift, compatible_disabled_ids, effective_bindings, parse_chord,
        validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    let def = catalog
        .iter()
        .find(|def| def.id == "toggle-orientation")
        .expect("row");
    assert_eq!(def.defaults, &["Win+O"]);
    assert!(def.implemented);
    assert!(!binding_wants_shift(def));
    assert!(!binding_wants_ctrl(def));
    assert_eq!(binding_action(def), ChordAction::Orientation);
    assert_eq!(binding_canonical_vk(def), Some(0x4F));
    let conflict = def.conflict.expect("orientation-lock text");
    assert!(
        conflict.contains("orientation lock") && conflict.contains("takeover"),
        "{conflict}"
    );
    // Exact modifiers: only unshifted Win+O matches the arm.
    let parsed = parse_chord("Win+O").expect("parse");
    assert!(binding_modifiers_ok(
        def,
        parsed.shift,
        parsed.ctrl,
        parsed.alt
    ));
    for chord in ["Win+Shift+O", "Win+Ctrl+O", "Win+Alt+O"] {
        let parsed = parse_chord(chord).expect("parse");
        assert!(
            !binding_modifiers_ok(def, parsed.shift, parsed.ctrl, parsed.alt),
            "{chord} must not match"
        );
    }
    // Authentic keeps; Compatible disables.
    let effective = effective_bindings(&Settings::default());
    let row = effective
        .iter()
        .find(|row| row.id == "toggle-orientation")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+O".to_owned()]);
    assert!(compatible_disabled_ids().contains(&"toggle-orientation"));
    let mut settings = Settings::default();
    apply_preset(&mut settings, Preset::Compatible);
    assert_eq!(
        settings
            .bindings
            .get("toggle-orientation")
            .map(|b| &b.state),
        Some(&BindingState::Disabled)
    );
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "toggle-orientation")
        .expect("row");
    assert!(!row.active && !row.effective);
    // Disable passes through; rebind to a fresh unshifted chord validates
    // and reports the rebound chord's own conflict.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-orientation".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-orientation".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_ok());
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "toggle-orientation")
        .expect("row");
    assert_eq!(row.chords, vec!["Win+U".to_owned()]);
    // Wrong-arm rebinds refuse.
    for chord in ["Win+Shift+U", "Win+Ctrl+U", "Win+Alt+U"] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "toggle-orientation".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(chord.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{chord} refuses");
    }
    // Duplicate active chord refuses.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-orientation".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+M".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
}

#[test]
fn item2_rebind_arms_and_duplicates_refuse() {
    use tiler_windows::settings::{Settings, validate_settings};
    // Stay digits need Win+Shift; relative rows need Win+Ctrl+Shift.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-1".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+F6".to_owned()),
        },
    );
    let err = validate_settings(&settings).expect_err("wrong arm refuses");
    assert!(err.to_string().contains("Win+Shift"), "{err}");
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-stay-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+F6".to_owned()),
        },
    );
    let err = validate_settings(&settings).expect_err("missing Ctrl refuses");
    assert!(err.to_string().contains("Win+Ctrl+Shift"), "{err}");
    // Rebinding a stay row onto a follow default chord refuses (duplicate
    // active chord); two rows onto one fresh chord refuse the second.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-1".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+1".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    let mut settings = Settings::default();
    for id in ["stay-workspace-1", "stay-workspace-2"] {
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Shift+F6".to_owned()),
            },
        );
    }
    assert!(validate_settings(&settings).is_err());
    // Valid stay rebinds on fresh chords pass, including two stays sharing
    // one VK across distinct digit slots.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-1".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+F6".to_owned()),
        },
    );
    settings.bindings.insert(
        "send-stay-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F7".to_owned()),
        },
    );
    validate_settings(&settings).expect("fresh stay rebinds validate");
}

#[test]
fn item2_numbered_stay_accepts_ctrl_shift_arm() {
    use tiler_windows::settings::{
        Settings, binding_arm_text, binding_catalog, binding_modifiers_ok, build_remap,
        effective_bindings, validate_settings,
    };
    // Backlog journey example: stay-workspace-2 rebound to Win+Ctrl+Shift+F6
    // validates; the plain Win+Shift arm keeps working (see the previous
    // test). Unshifted and Alt chords refuse with the dual-arm message.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    validate_settings(&settings).expect("ctrl+shift stay validates");
    let catalog = binding_catalog();
    let def = catalog
        .iter()
        .find(|def| def.id == "stay-workspace-2")
        .expect("stay row");
    assert!(binding_modifiers_ok(def, true, false, false));
    assert!(binding_modifiers_ok(def, true, true, false));
    assert!(!binding_modifiers_ok(def, false, false, false));
    assert!(!binding_modifiers_ok(def, true, false, true));
    assert!(!binding_modifiers_ok(def, true, true, true));
    assert_eq!(binding_arm_text(def), "Win+Shift or Win+Ctrl+Shift");
    // Digit follow rows still refuse Ctrl through the same helper.
    let follow = catalog
        .iter()
        .find(|def| def.id == "workspace-send-2")
        .expect("follow row");
    assert!(!binding_modifiers_ok(follow, true, true, false));
    // Remap carries the full Ctrl+Shift modifiers into the explicit stay
    // action at the shared digit slot; the effective row reports the honest
    // unknown-ownership conflict of the rebound chord.
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(
        (
            remap[0].from_vk,
            remap[0].from_shift,
            remap[0].from_ctrl,
            remap[0].from_alt
        ),
        (0x75, true, true, false)
    );
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "stay-workspace-2")
        .expect("row");
    assert!(row.active && row.effective);
    let conflict = row.conflict.expect("honest rebound conflict");
    assert!(
        conflict.contains("ownership unknown") && conflict.contains("unproven live"),
        "{conflict}"
    );
    // Wrong arms refuse naming both stay arms; Alt never arms stay.
    for chord in ["Win+F6", "Win+Alt+Shift+F6", "Win+Ctrl+F6"] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "stay-workspace-2".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(chord.to_owned()),
            },
        );
        let err = validate_settings(&settings).expect_err("wrong arm refuses");
        assert!(
            err.to_string().contains("Win+Shift or Win+Ctrl+Shift"),
            "{chord}: {err}"
        );
    }
    // Full-modifier duplicate detection: the same physical chord rebound
    // twice refuses, while Shift and Ctrl+Shift variants coexist.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    settings.bindings.insert(
        "send-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-1".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+F6".to_owned()),
        },
    );
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    validate_settings(&settings).expect("shift vs ctrl+shift coexist");
}

#[test]
fn item2_keep_empty_is_unbound_not_disabled() {
    use tiler_windows::settings::{Settings, effective_bindings};
    // Keep (absent override) on an unbound stay row: active with no chord,
    // not effective, with the bindable reason - distinctly not disabled.
    let effective = effective_bindings(&Settings::default());
    let row = effective
        .iter()
        .find(|row| row.id == "stay-workspace-1")
        .expect("row");
    assert!(row.active);
    assert!(row.chords.is_empty());
    assert!(!row.effective);
    assert_eq!(row.reason, "unbound: bindable");
    // Disabled reads differently: inactive with the pass-through reason.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-1".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "stay-workspace-1")
        .expect("row");
    assert!(!row.active);
    assert_eq!(row.reason, "disabled: passes through natively");
}
