#![forbid(unsafe_code)]
use super::*;

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
