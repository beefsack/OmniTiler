#![forbid(unsafe_code)]
use tiler_windows::settings::{
    BindingSetting, BindingState, Preset, Settings, apply_preset, binding_catalog, build_disabled,
    build_remap, compatible_disabled_ids, effective_bindings, parse_chord, render_chord,
    validate_settings,
};
use tiler_windows::snapkey::{ChordAction, Classified, KeyboardConfig, SnapClassify};

const VK_LWIN: u32 = 91;
const VK_SHIFT: u32 = 16;
const VK_CONTROL: u32 = 17;
const VK_MENU: u32 = 18;
const VK_G: u32 = 0x47;
const VK_F11: u32 = 0x7A;
const VK_U: u32 = 0x55;

fn takeover() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }
}

fn win_down(m: &mut SnapClassify) {
    assert_eq!(SnapClassify::push(m, VK_LWIN, false, true, false), None);
}

fn ctrl_down(m: &mut SnapClassify) {
    assert_eq!(SnapClassify::push(m, VK_CONTROL, false, true, false), None);
}

fn compatible_settings() -> Settings {
    let mut settings = Settings::default();
    apply_preset(&mut settings, Preset::Compatible);
    settings
}

fn apply_tables(m: &mut SnapClassify, settings: &Settings) {
    m.set_remap(build_remap(settings));
    m.set_disabled(build_disabled(settings));
}

#[test]
fn compatible_rebinds_float_and_fullscreen_to_ctrl() {
    let settings = compatible_settings();
    assert!(validate_settings(&settings).is_ok());
    // The two Xbox chords leave the disabled set and ride as Ctrl rebinds.
    assert!(!compatible_disabled_ids().contains(&"toggle-float"));
    assert!(!compatible_disabled_ids().contains(&"toggle-fullscreen"));
    assert_eq!(compatible_disabled_ids().len(), 39);
    for id in ["toggle-float", "toggle-fullscreen"] {
        let entry = settings.bindings.get(id).expect("rebind entry");
        assert_eq!(entry.state, BindingState::Rebind);
    }
    let float_chord = settings
        .bindings
        .get("toggle-float")
        .and_then(|b| b.chord.clone())
        .expect("float chord");
    let fs_chord = settings
        .bindings
        .get("toggle-fullscreen")
        .and_then(|b| b.chord.clone())
        .expect("fullscreen chord");
    assert_eq!(
        render_chord(&parse_chord(&float_chord).expect("parse")),
        "Win+Ctrl+G"
    );
    assert_eq!(
        render_chord(&parse_chord(&fs_chord).expect("parse")),
        "Win+Ctrl+F11"
    );
    let effective = effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|r| r.id == "toggle-float")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+Ctrl+G".to_owned()]);
    let row = effective
        .iter()
        .find(|r| r.id == "toggle-fullscreen")
        .expect("row");
    assert!(row.active && row.effective);
    assert_eq!(row.chords, vec!["Win+Ctrl+F11".to_owned()]);
    // Old defaults pass through; the Ctrl chords route via remap.
    let disabled = build_disabled(&settings);
    assert!(
        disabled
            .iter()
            .any(|d| d.vk == VK_G && !d.shift && !d.ctrl && !d.alt)
    );
    assert!(
        disabled
            .iter()
            .any(|d| d.vk == VK_F11 && !d.shift && !d.ctrl && !d.alt)
    );
    let remap = build_remap(&settings);
    let float_remap = remap
        .iter()
        .find(|e| e.action == ChordAction::Float)
        .expect("float remap");
    assert_eq!(
        (
            float_remap.from_vk,
            float_remap.from_shift,
            float_remap.from_ctrl,
            float_remap.from_alt,
            float_remap.to_vk
        ),
        (VK_G, false, true, false, VK_G)
    );
    let fs_remap = remap
        .iter()
        .find(|e| e.action == ChordAction::Fullscreen)
        .expect("fs remap");
    assert_eq!(
        (
            fs_remap.from_vk,
            fs_remap.from_shift,
            fs_remap.from_ctrl,
            fs_remap.from_alt,
            fs_remap.to_vk
        ),
        (VK_F11, false, true, false, VK_F11)
    );
    // Compatible clears every tray warning (Ctrl chords carry no
    // incomplete-containment note).
    assert!(tiler_windows::tray::unresolved_conflicts(&settings, true, false).is_empty());
    // Authentic is untouched: plain Win+G / Win+F11 kept.
    let authentic = Settings::default();
    let effective = effective_bindings(&authentic);
    let row = effective
        .iter()
        .find(|r| r.id == "toggle-float")
        .expect("row");
    assert_eq!(row.chords, vec!["Win+G".to_owned()]);
    let row = effective
        .iter()
        .find(|r| r.id == "toggle-fullscreen")
        .expect("row");
    assert_eq!(row.chords, vec!["Win+F11".to_owned()]);
}

#[test]
fn ctrl_validation_accepts_only_float_and_fullscreen() {
    for (id, chord) in [
        ("toggle-float", "Win+Ctrl+G"),
        ("toggle-fullscreen", "Win+Ctrl+F11"),
    ] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(chord.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok(), "{id} {chord}");
    }
    // No widening: sticky, maximize, orientation, tiling and launchers
    // still refuse Ctrl.
    for (id, chord) in [
        ("toggle-sticky", "Win+Ctrl+Shift+G"),
        ("toggle-sticky", "Win+Ctrl+G"),
        ("toggle-maximize", "Win+Ctrl+M"),
        ("toggle-orientation", "Win+Ctrl+O"),
        ("toggle-workspace-tiling", "Win+Ctrl+Y"),
        ("launch-browser", "Win+Ctrl+B"),
        ("launch-terminal", "Win+Ctrl+T"),
        ("toggle-fullscreen", "Win+Ctrl+Shift+F11"),
        ("toggle-fullscreen", "Win+Ctrl+Alt+F11"),
        ("toggle-float", "Win+Ctrl+Alt+G"),
        ("toggle-float", "Win+Ctrl+Shift+G"),
    ] {
        let mut settings = Settings::default();
        settings.bindings.insert(
            id.to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some(chord.to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err(), "{id} {chord}");
    }
    // Duplicate Ctrl chords still refuse.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "toggle-float".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+G".to_owned()),
        },
    );
    settings.bindings.insert(
        "toggle-fullscreen".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+G".to_owned()),
        },
    );
    assert!(validate_settings(&settings).is_err());
    // Default catalog occupies neither candidate.
    let mut seen = Vec::new();
    for def in binding_catalog() {
        for default in def.defaults {
            seen.push(render_chord(&parse_chord(default).expect("catalog parses")));
        }
    }
    assert!(!seen.contains(&"Win+Ctrl+G".to_owned()));
    assert!(!seen.contains(&"Win+Ctrl+F11".to_owned()));
}

#[test]
fn ctrl_toggle_down_repeat_up_consumes_with_mask() {
    let settings = compatible_settings();
    // Float Ctrl+Win+G.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_G, false, true, false).expect("float down") {
        Classified::Float(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("expected float, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_G, false, true, false).expect("float repeat") {
        Classified::Float(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected float repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_G, true, true, false).expect("float up") {
        Classified::Float(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected float up, got {other:?}"),
    }
    // Fullscreen Ctrl+Win+F11.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_F11, false, true, false).expect("fs down") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("expected fullscreen, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_F11, false, true, false).expect("fs repeat") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected fs repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_F11, true, true, false).expect("fs up") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected fs up, got {other:?}"),
    }
}

#[test]
fn ctrl_toggle_early_win_and_ctrl_release_keep_pairing() {
    let settings = compatible_settings();
    // Early Win release: hold still pairs by origin.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_G, false, true, false).expect("down") {
        Classified::Float(intent) => assert!(intent.consumed),
        other => panic!("{other:?}"),
    }
    assert_eq!(SnapClassify::push(&mut m, VK_LWIN, true, true, false), None);
    match SnapClassify::push(&mut m, VK_G, true, true, false).expect("paired up") {
        Classified::Float(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("{other:?}"),
    }
    // Ctrl released mid-hold: repeat stays swallowed, up still closes.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_F11, false, true, false).expect("down") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        SnapClassify::push(&mut m, VK_CONTROL, true, true, false),
        None
    );
    match SnapClassify::push(&mut m, VK_F11, false, true, false).expect("repeat") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("{other:?}"),
    }
    match SnapClassify::push(&mut m, VK_F11, true, true, false).expect("up") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ctrl_toggle_wrong_modifiers_and_takeover_off_pass_through() {
    let settings = compatible_settings();
    // Shifted or Alt-modified Ctrl+G is not the rebound chord.
    for mods in [VK_SHIFT, VK_MENU] {
        let mut m = SnapClassify::new(takeover());
        apply_tables(&mut m, &settings);
        win_down(&mut m);
        ctrl_down(&mut m);
        assert_eq!(SnapClassify::push(&mut m, mods, false, true, false), None);
        assert_eq!(SnapClassify::push(&mut m, VK_G, false, true, false), None);
        assert_eq!(SnapClassify::push(&mut m, VK_G, true, true, false), None);
        assert_eq!(SnapClassify::push(&mut m, mods, true, true, false), None);
    }
    // Shifted Ctrl+F11 refuses too.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    assert_eq!(
        SnapClassify::push(&mut m, VK_SHIFT, false, true, false),
        None
    );
    assert_eq!(SnapClassify::push(&mut m, VK_F11, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_F11, true, true, false), None);
    // Takeover off: the Ctrl arms pass through without consuming.
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_G, false, true, false).expect("logged") {
        Classified::Float(intent) => assert!(!intent.consumed && !intent.announce),
        other => panic!("{other:?}"),
    }
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    apply_tables(&mut m, &settings);
    win_down(&mut m);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_F11, false, true, false).expect("logged") {
        Classified::Fullscreen(intent) => assert!(!intent.consumed && !intent.announce),
        other => panic!("{other:?}"),
    }
}

#[test]
fn fresh_unbound_ctrl_chords_and_old_defaults_stay_unowned() {
    let compatible = compatible_settings();
    // Fresh Ctrl+Win+U (never bound) passes through even with the
    // Compatible tables installed: no new capture.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &compatible);
    win_down(&mut m);
    ctrl_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_U, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_U, true, true, false), None);
    // Compatible suppresses the old defaults: plain Win+G / Win+F11 pass.
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &compatible);
    win_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_G, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_G, true, true, false), None);
    let mut m = SnapClassify::new(takeover());
    apply_tables(&mut m, &compatible);
    win_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_F11, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_F11, true, true, false), None);
    // Authentic without rebinds: fresh Ctrl+Win+G / Ctrl+Win+F11 pass
    // through (no direct arm), while the authentic chords still consume.
    let authentic = Settings::default();
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&authentic));
    m.set_disabled(build_disabled(&authentic));
    win_down(&mut m);
    ctrl_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_G, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_G, true, true, false), None);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&authentic));
    m.set_disabled(build_disabled(&authentic));
    win_down(&mut m);
    ctrl_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_F11, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_F11, true, true, false), None);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&authentic));
    m.set_disabled(build_disabled(&authentic));
    win_down(&mut m);
    match SnapClassify::push(&mut m, VK_G, false, true, false).expect("authentic float") {
        Classified::Float(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("{other:?}"),
    }
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&authentic));
    m.set_disabled(build_disabled(&authentic));
    win_down(&mut m);
    match SnapClassify::push(&mut m, VK_F11, false, true, false).expect("authentic fs") {
        Classified::Fullscreen(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("{other:?}"),
    }
}
