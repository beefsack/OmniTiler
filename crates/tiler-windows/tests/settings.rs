use std::path::PathBuf;
use tiler_windows::settings::{
    BindingSetting, BindingState, LoadOutcome, PollOutcome, Preset, Settings, build_disabled,
    load_from_dir, parse_chord, poll_for_change, save_to_dir,
};
use tiler_windows::snapkey::{ChordDisable, ChordRemap, Classified, KeyboardConfig, SnapClassify};
use tiler_windows::tiling::{
    CliOverrides, TileOptions, apply_cli_overrides, parse_tile_args_from,
    parse_tile_args_from_with_overrides, tile_options_defaults,
};

static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(name: &str) -> Self {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tiler-settings-{name}-{}-{t}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temp dir");
        Self { path }
    }

    fn file(&self) -> PathBuf {
        self.path.join("settings.json")
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn takeover() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }
}

const VK_LWIN: u32 = 91;
const VK_SHIFT: u32 = 16;
const VK_H: u32 = 0x48;
const VK_U: u32 = 0x55;
const VK_G: u32 = 0x47;
const VK_Q: u32 = 0x51;

#[test]
fn store_roundtrip_bumps_revision_and_leaves_no_pending() {
    let temp = Temp::new("roundtrip");
    let mut settings = Settings::default();
    save_to_dir(&temp.path, &mut settings).expect("save");
    assert_eq!(settings.revision, 1, "first save starts at revision 1");
    let LoadOutcome::Loaded(back) = load_from_dir(&temp.path) else {
        panic!("reloads");
    };
    assert_eq!(back, settings);
    save_to_dir(&temp.path, &mut settings).expect("resave");
    assert_eq!(settings.revision, 2);
    let entries: Vec<String> = std::fs::read_dir(&temp.path)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["settings.json".to_owned()]);
}

#[test]
fn malformed_file_reports_and_poll_keeps_last_good_without_rewrite() {
    let temp = Temp::new("malformed");
    let mut settings = Settings::default();
    save_to_dir(&temp.path, &mut settings).expect("save");
    std::fs::write(temp.file(), b"{not json").expect("corrupt");
    assert!(matches!(load_from_dir(&temp.path), LoadOutcome::Invalid(_)));
    // Live poll keeps last-good with a degraded status; bytes untouched.
    let live = tiler_windows::settings::LiveSettings::fresh(settings, None);
    let outcome = poll_for_change(&temp.path, &live);
    let PollOutcome::InvalidKept(reason) = outcome else {
        panic!("keeps last-good, got {outcome:?}");
    };
    assert!(reason.contains("malformed"), "{reason}");
    assert_eq!(std::fs::read(temp.file()).expect("bytes"), b"{not json");
    assert_eq!(live.status, "saved:1");
}

#[test]
fn oversize_and_version_gate_refuse() {
    let temp = Temp::new("bounds");
    let big = vec![b'x'; 70_000];
    std::fs::write(temp.file(), &big).expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(tiler_windows::settings::SettingsError::TooLarge)
    ));
    let settings = Settings {
        v: 2,
        ..Settings::default()
    };
    let text = serde_json::to_string(&settings).expect("json");
    std::fs::write(temp.file(), text).expect("write");
    assert!(matches!(
        load_from_dir(&temp.path),
        LoadOutcome::Invalid(tiler_windows::settings::SettingsError::UnsupportedVersion)
    ));
}

#[test]
fn save_rejects_invalid_without_touching_good_file() {
    let temp = Temp::new("save-guard");
    let mut settings = Settings::default();
    save_to_dir(&temp.path, &mut settings).expect("save");
    let good_bytes = std::fs::read(temp.file()).expect("bytes");
    settings.core.inner_gap = 65;
    assert!(save_to_dir(&temp.path, &mut settings).is_err());
    assert_eq!(std::fs::read(temp.file()).expect("bytes"), good_bytes);
    // Invalid on-disk content also blocks save (no blind overwrite).
    std::fs::write(temp.file(), b"[]").expect("corrupt");
    let mut fresh = Settings::default();
    assert!(save_to_dir(&temp.path, &mut fresh).is_err());
}

#[test]
fn cli_explicit_switches_override_saved_values() {
    let mut settings = Settings::default();
    settings.core.inner_gap = 4;
    settings.core.outer_gap = 12;
    settings.core.border.enabled = false;
    settings.core.keyboard.takeover = false;
    let base = tiler_windows::settings::tile_options_from_settings(&settings);
    assert_eq!(base.inner_gap, 4);
    assert_eq!(base.outer_gap, 12);
    assert!(!base.border.enabled);
    assert!(base.no_keyboard_snap_takeover);
    // Saved values flow through untouched when no switch is given.
    let parsed = parse_tile_args_from(&strings(&["--user-start"]), &base).expect("parsed");
    assert_eq!(parsed.inner_gap, 4);
    assert_eq!(parsed.outer_gap, 12);
    assert!(!parsed.border.enabled);
    assert!(parsed.no_keyboard_snap_takeover);
    // Explicit switches override exactly their lane.
    let parsed = parse_tile_args_from(
        &strings(&[
            "--user-start",
            "--inner-gap",
            "10",
            "--active-border-width",
            "5",
            "--allow-win-l",
        ]),
        &base,
    )
    .expect("parsed");
    assert_eq!(parsed.inner_gap, 10);
    assert_eq!(parsed.outer_gap, 12);
    assert_eq!(parsed.border.style.width, 5.0);
    assert!(!parsed.border.enabled);
    assert!(parsed.allow_win_l);
    // The user-start fence never comes from saved settings.
    assert!(parse_tile_args_from(&strings(&[]), &base).is_err());
    // Gap range still refuses.
    assert!(parse_tile_args_from(&strings(&["--user-start", "--outer-gap", "65"]), &base).is_err());
    // Compiled-in defaults are untouched by the settings base.
    let defaults: TileOptions = tile_options_defaults();
    assert_eq!((defaults.inner_gap, defaults.outer_gap), (8, 8));
}

#[test]
fn cli_overrides_stay_authoritative_across_unrelated_file_apply() {
    // Startup: file says gaps 4/12 with takeover on; CLI pins inner gap 10
    // and the keyboard off switch.
    let mut settings = Settings::default();
    settings.core.inner_gap = 4;
    settings.core.outer_gap = 12;
    let base = tiler_windows::settings::tile_options_from_settings(&settings);
    let (options, cli) = parse_tile_args_from_with_overrides(
        &strings(&[
            "--user-start",
            "--inner-gap",
            "10",
            "--no-keyboard-snap-takeover",
        ]),
        &base,
    )
    .expect("parsed");
    assert_eq!(options.inner_gap, 10);
    assert!(options.no_keyboard_snap_takeover);
    assert_eq!(
        cli,
        CliOverrides {
            inner_gap: Some(10),
            no_keyboard_snap_takeover: Some(true),
            ..CliOverrides::default()
        }
    );
    // Unrelated file apply: gaps back to 8/8, takeover back on, border 5.
    settings.core.inner_gap = 8;
    settings.core.outer_gap = 8;
    settings.core.border.width = 5.0;
    let mut rebased = tiler_windows::settings::tile_options_from_settings(&settings);
    apply_cli_overrides(&mut rebased, &cli);
    assert_eq!(rebased.inner_gap, 10, "CLI gap stays authoritative");
    assert_eq!(rebased.outer_gap, 8, "unrelated file lane applies");
    assert!(
        rebased.no_keyboard_snap_takeover,
        "CLI off switch stays authoritative"
    );
    assert_eq!(
        rebased.border.style.width, 5.0,
        "unrelated file lane applies"
    );
}

#[test]
fn rebind_routes_single_arm_and_old_default_passes_through() {
    // focus-left rebound Win+H -> Win+U: unshifted Win+U focuses left, the
    // shifted chord on the same key passes through (no unintended move),
    // and the rebound-away Win+H passes through too.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    let remap = tiler_windows::settings::build_remap(&settings);
    assert_eq!(
        remap,
        vec![ChordRemap {
            from_vk: VK_U,
            from_shift: false,
            from_ctrl: false,
            from_alt: false,
            action: tiler_windows::snapkey::ChordAction::Directional,
            to_vk: VK_H
        },]
    );
    let disabled = build_disabled(&settings);
    assert_eq!(
        disabled,
        vec![ChordDisable {
            vk: VK_H,
            shift: false,
            ctrl: false,
            alt: false
        }]
    );
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(remap);
    machine.set_disabled(disabled);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("down");
    let Classified::Snap(intent) = down else {
        panic!("rebound U classifies as the directional arm");
    };
    assert!(intent.consumed && intent.announce);
    assert_eq!(intent.direction, tiler_core::directional::Direction::Left);
    assert_eq!(intent.op, tiler_windows::snapkey::SnapOp::Focus);
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    // Shifted Win+U is not the rebound: passes through untracked.
    let mut shifted = SnapClassify::new(takeover());
    shifted.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    shifted.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut shifted, VK_LWIN, false, true, false);
    SnapClassify::push(&mut shifted, VK_SHIFT, false, true, false);
    assert_eq!(
        SnapClassify::push(&mut shifted, VK_U, false, true, false),
        None,
        "shifted rebound chord passes through: single-arm routing"
    );
    SnapClassify::push(&mut shifted, VK_SHIFT, true, true, false);
    // The rebound-away Win+H passes through on the same machine.
    assert_eq!(
        SnapClassify::push(&mut shifted, VK_H, false, true, false),
        None,
        "rebound-away default passes through"
    );
    assert_eq!(
        SnapClassify::push(&mut shifted, VK_H, true, true, false),
        None,
        "rebound-away pair leaves no hold"
    );
}

#[test]
fn focus_right_safe_rebind_needs_no_win_l_opt_in() {
    // focus-right rebound onto Win+U: the physical chord is not the OS lock
    // chord, so it routes without `allow_win_l`, while physical Win+L still
    // passes through.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-right".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(tiler_windows::settings::build_remap(&settings));
    machine.set_disabled(build_disabled(&settings));
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("down");
    let Classified::Snap(intent) = down else {
        panic!("safe rebound routes to focus-right");
    };
    assert!(intent.consumed && intent.announce);
    assert_eq!(intent.direction, tiler_core::directional::Direction::Right);
    assert_eq!(intent.op, tiler_windows::snapkey::SnapOp::Focus);
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("up");
    assert!(up.consumed());
    // Physical Win+L stays gated without opt-in on the same machine.
    assert_eq!(
        SnapClassify::push(&mut machine, 0x4C, false, true, false),
        None,
        "physical Win+L still passes through without opt-in"
    );
}

#[test]
fn takeover_off_preserves_consumed_release_and_mask() {
    let mut machine = SnapClassify::new(takeover());
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("down");
    assert!(down.consumed());
    // Live disable: the owner publishes the policy (no hold clearing) and
    // drains the queue (here: the queue drain is owner-side; the classifier
    // half must still swallow the consumed pair and keep the Start mask, so
    // no orphan key-up reaches the OS and no naked Win tap opens Start).
    machine.set_enabled(false);
    let up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("up");
    assert!(
        up.consumed() && !up.announce(),
        "paired key-up rides the stored consumed verdict"
    );
    // A fresh chord while off passes through with its pair, never consuming.
    let mut off = SnapClassify::new(KeyboardConfig::disabled());
    SnapClassify::push(&mut off, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut off, VK_H, false, true, false).expect("down");
    assert!(!down.consumed() && !down.announce());
    let up = SnapClassify::push(&mut off, VK_H, true, true, false).expect("up");
    assert!(!up.consumed());
}

#[test]
fn mid_hold_remap_removal_pins_pairing_and_swallows() {
    // Win+U rebound to H, removed mid-hold: the repeat stays swallowed
    // without dispatching, the pair still closes consumed, and a fresh
    // press afterwards passes through (no orphan slot, no leaked up).
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("down");
    assert!(down.consumed() && down.announce());
    assert!(machine.key_is_down(VK_U));
    machine.set_remap(Vec::new());
    // Pinned repeat: swallowed, never re-dispatched under the new tables.
    let repeat = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("repeat");
    assert!(repeat.consumed() && !repeat.announce());
    // Pinned pair closes consumed even though the mapping is gone.
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    assert!(!machine.key_is_down(VK_U));
    // Fresh press under the new tables passes through untracked.
    assert_eq!(
        SnapClassify::push(&mut machine, VK_U, false, true, false),
        None
    );
    assert_eq!(
        SnapClassify::push(&mut machine, VK_U, true, true, false),
        None
    );
}

#[test]
fn mid_hold_disable_gates_repeat_dispatch_but_keeps_pair() {
    // Consumed Win+H, default disabled mid-hold: the repeat stays swallowed
    // without dispatching (no stale action), the pair still closes consumed.
    let mut machine = SnapClassify::new(takeover());
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("down");
    assert!(down.consumed() && down.announce());
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    let repeat = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed() && !repeat.announce());
    let up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    // Fresh press under the new tables passes through untracked.
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, false, true, false),
        None
    );
}

#[test]
fn keep_equivalent_rebind_routes_natively() {
    // focus-left rebound onto its own Win+H: valid, no tables, native focus.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+H".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    assert!(tiler_windows::settings::build_remap(&settings).is_empty());
    assert!(build_disabled(&settings).is_empty());
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(tiler_windows::settings::build_remap(&settings));
    machine.set_disabled(build_disabled(&settings));
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("down");
    let Classified::Snap(intent) = down else {
        panic!("native focus-left still routes");
    };
    assert!(intent.consumed && intent.announce);
    assert_eq!(intent.op, tiler_windows::snapkey::SnapOp::Focus);
}

#[test]
fn remap_up_survives_mid_hold_shift_flip() {
    // Sticky rebound onto Win+Shift+Q: a Shift release before key-up must
    // still close the pair instead of orphaning a stuck hold. Single-polarity:
    // the custom chord matches the sticky (shifted) arm.
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_Q,
        from_shift: true,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Sticky,
        to_vk: VK_G,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_Q, false, true, false).expect("down");
    let Classified::Sticky(intent) = down else {
        panic!("rebound Q classifies sticky");
    };
    assert!(intent.consumed && intent.announce);
    SnapClassify::push(&mut machine, VK_SHIFT, true, true, false);
    let up = SnapClassify::push(&mut machine, VK_Q, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    assert_eq!(
        SnapClassify::push(&mut machine, VK_Q, true, true, false),
        None,
        "hold closed: no orphan"
    );
}

#[test]
fn rebound_collision_shifted_native_never_repeats_owner() {
    // focus-left rebound Win+U (canonical H) with native shifted
    // Win+Shift+H (move-left) kept: Win, U down (consumed focus), Shift,
    // H down is swallowed as a safe consumed refusal (inheriting the live
    // owner's shape, never dispatched), never U's repeat. Both paired ups
    // stay consumed with no stuck slot.
    use tiler_windows::snapkey::{SnapEdge, SnapOp};
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
    let Classified::Snap(focus) = down else {
        panic!("rebound U classifies directional");
    };
    assert_eq!((focus.op, focus.edge), (SnapOp::Focus, SnapEdge::Down));
    assert!(focus.consumed && focus.announce);
    SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
    let hit = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("H down");
    let Classified::Snap(refused) = hit else {
        panic!("colliding H classifies directional");
    };
    // Never U's repeat: a fresh safe refusal down, consumed and never
    // dispatched. Old shared-slot code returned U's Focus repeat.
    assert_eq!(
        refused.edge,
        SnapEdge::Down,
        "colliding H is a fresh down, not U's repeat"
    );
    assert!(refused.consumed && !refused.announce);
    let h_up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("H up");
    assert!(h_up.consumed() && !h_up.announce());
    let u_up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
    assert!(u_up.consumed() && !u_up.announce());
    assert!(!machine.key_is_down(VK_U));
    assert!(!machine.key_is_down(VK_H));
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, true, true, false),
        None,
        "no stuck slot: extra up passes"
    );
    assert_eq!(
        SnapClassify::push(&mut machine, VK_U, true, true, false),
        None,
        "no stuck slot: extra up passes"
    );
}

#[test]
fn suppressed_rebound_away_up_never_steals_owner_pair() {
    // First U consumed down, suppressed rebound-away H down passes, H up
    // must PASS untracked without closing U's slot; U up still consumed.
    // Old unpinned-up fallback mapped H up to canonical H and stole U's hold.
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
    assert!(down.consumed());
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, false, true, false),
        None,
        "rebound-away H down passes suppressed"
    );
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, true, true, false),
        None,
        "unrelated H up passes without stealing U's slot"
    );
    assert!(
        machine.key_is_down(VK_U),
        "U hold survives the foreign H tap"
    );
    let u_up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
    assert!(u_up.consumed() && !u_up.announce());
    assert!(!machine.key_is_down(VK_U));
}

#[test]
fn same_physical_rebound_pins_across_shift_flip() {
    // Same physical U rebound to H: Shift flip mid-hold keeps the pinned
    // Focus op, repeats stay swallowed without dispatching, pair closes
    // consumed. Per-binding single-polarity holds: shifted U never routes.
    use tiler_windows::snapkey::{SnapEdge, SnapOp};
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
    let Classified::Snap(focus) = down else {
        panic!("rebound U classifies directional");
    };
    assert_eq!(focus.op, SnapOp::Focus);
    SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
    let repeat = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U repeat");
    let Classified::Snap(rep) = repeat else {
        panic!("pinned U repeat classifies directional");
    };
    assert_eq!((rep.op, rep.edge), (SnapOp::Focus, SnapEdge::Repeat));
    assert!(rep.consumed && !rep.announce);
    SnapClassify::push(&mut machine, VK_SHIFT, true, true, false);
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
    assert!(up.consumed() && !up.announce());
    assert!(!machine.key_is_down(VK_U));
}

#[test]
fn three_colliders_pair_in_any_release_order_across_remap_change() {
    // Arbitrary bare-API mapping: two rebound physicals plus the native key
    // share one canonical. Owner plus both colliders each close consumed in
    // any release order with no stuck slot and no OS leak, even when the
    // mapping vanishes mid-hold (colliders ride their cached verdict, the
    // owner rides its pin).
    const VK_I: u32 = 0x49;
    let tables = || {
        (
            vec![
                ChordRemap {
                    from_vk: VK_U,
                    from_shift: false,
                    from_ctrl: false,
                    from_alt: false,
                    action: tiler_windows::snapkey::ChordAction::Directional,
                    to_vk: VK_H,
                },
                ChordRemap {
                    from_vk: VK_I,
                    from_shift: false,
                    from_ctrl: false,
                    from_alt: false,
                    action: tiler_windows::snapkey::ChordAction::Directional,
                    to_vk: VK_H,
                },
            ],
            vec![ChordDisable {
                vk: VK_H,
                shift: false,
                ctrl: false,
                alt: false,
            }],
        )
    };
    for owner_first in [false, true] {
        let mut machine = SnapClassify::new(takeover());
        let (remap, disabled) = tables();
        machine.set_remap(remap);
        machine.set_disabled(disabled);
        SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
        let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
        assert!(down.consumed() && down.announce());
        // Native H routes only shifted here (unshifted H is suppressed);
        // rebound I routes only unshifted (single-polarity entries).
        SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
        let h = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("H collider");
        assert!(h.consumed() && !h.announce());
        SnapClassify::push(&mut machine, VK_SHIFT, true, true, false);
        let i = SnapClassify::push(&mut machine, VK_I, false, true, false).expect("I collider");
        assert!(i.consumed() && !i.announce());
        assert!(machine.key_is_down(VK_U));
        assert!(machine.key_is_down(VK_H));
        assert!(machine.key_is_down(VK_I));
        // Mapping vanishes mid-hold: pairs must still close consumed.
        machine.set_remap(Vec::new());
        if owner_first {
            let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
            assert!(up.consumed() && !up.announce());
        }
        let up = SnapClassify::push(&mut machine, VK_I, true, true, false).expect("I up");
        assert!(up.consumed() && !up.announce());
        let up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("H up");
        assert!(up.consumed() && !up.announce());
        if !owner_first {
            let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
            assert!(up.consumed() && !up.announce());
        }
        assert!(!machine.key_is_down(VK_U));
        assert!(!machine.key_is_down(VK_H));
        assert!(!machine.key_is_down(VK_I));
        assert_eq!(
            SnapClassify::push(&mut machine, VK_U, true, true, false),
            None,
            "no leak: extra U up passes"
        );
    }
}

#[test]
fn compatible_preset_disables_os_conflicting_rows() {
    let mut settings = Settings::default();
    let changed = tiler_windows::settings::apply_preset(&mut settings, Preset::Compatible);
    // Every OS-conflicting row: 8 focus + 4 move-arrow + 4 toggles + 20
    // workspace digits + 2 history arrows.
    assert_eq!(changed.len(), 38);
    assert!(changed.contains(&"toggle-float"));
    assert!(changed.contains(&"toggle-fullscreen"));
    assert!(changed.contains(&"toggle-orientation"));
    assert!(changed.contains(&"focus-left"));
    assert!(changed.contains(&"focus-left-arrow"));
    assert!(changed.contains(&"move-left-arrow"));
    assert!(changed.contains(&"workspace-select-1"));
    assert!(changed.contains(&"workspace-prev-left-arrow"));
    assert!(changed.contains(&"workspace-next-right-arrow"));
    let effective = tiler_windows::settings::effective_bindings(&settings);
    assert_eq!(effective.len(), 84);
    for row in &effective {
        if tiler_windows::settings::compatible_disabled_ids().contains(&row.id) {
            assert!(!row.active, "{}", row.id);
            assert!(!row.effective, "{}", row.id);
        }
    }
    // Conflict-free letter moves plus sticky stay active, plus the kept
    // history rows (Tab/letters/Up/Down) and the kept item 2 relative-send
    // follow rows (unknown ownership, no new Compatible disables).
    for id in [
        "move-left",
        "move-down",
        "move-up",
        "move-right",
        "toggle-sticky",
        "workspace-previous",
        "workspace-prev-h",
        "workspace-next-j",
        "send-prev-h",
        "send-next-j",
    ] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert!(row.active && row.effective, "{id}");
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
// + 8 relative stay) for 83 total, plus the item 4 orientation toggle for 84;
// follow defaults keep pending evidenced conflicts with honest
// unknown-ownership text (no new Compatible disables); stay rows are bindable
// unbound with Keep meaning unbound.

#[test]
fn catalog_exposes_item2_rows_with_exact_modifiers() {
    use tiler_windows::settings::{
        binding_action, binding_canonical_vk, binding_catalog, binding_wants_ctrl,
        binding_wants_shift,
    };
    use tiler_windows::snapkey::ChordAction;
    let catalog = binding_catalog();
    assert_eq!(catalog.len(), 84);
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
    assert_eq!(catalog.len(), 84);
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
