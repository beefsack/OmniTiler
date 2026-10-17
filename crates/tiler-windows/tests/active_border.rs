#![forbid(unsafe_code)]
use tiler_windows::active_border::{DEFAULT_COLOR_RGB, render_color};
use tiler_windows::tiling::{
    parse_shortcut_proof_args, parse_tile_args, parse_tile_proof_args, parse_workspace_proof_args,
    verify_proof_argv_consistency, verify_shortcut_proof_argv_consistency,
    verify_workspace_proof_argv_consistency,
};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn border_defaults_on_with_kde_accent_fallback() {
    let options = parse_tile_args(&strings(&["--user-start"])).expect("parsed");
    assert!(options.border.enabled);
    assert_eq!(options.border.style.width, 3.0);
    assert_eq!(options.border.style.gap, 0.0);
    assert_eq!(options.border.style.radius, 6.0);
    assert_eq!(options.border.style.color, DEFAULT_COLOR_RGB);
    assert_eq!(render_color(options.border.style.color), "#2a82da");
    assert!(options.border.style.use_theme);
}

#[test]
fn border_explicit_zero_radius_stays_square_on_all_commands() {
    // The 6.0 default never overwrites an explicit radius: `--active-border-radius 0`
    // keeps square corners reachable on every command.
    let tile =
        parse_tile_args(&strings(&["--user-start", "--active-border-radius", "0"])).expect("tile");
    assert_eq!(tile.border.style.radius, 0.0);
    let options = parse_tile_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--active-border-radius",
        "0",
    ]))
    .expect("tile-proof");
    assert_eq!(options.border.style.radius, 0.0);
    let options = parse_workspace_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--active-border-radius",
        "0",
    ]))
    .expect("workspace-proof");
    assert_eq!(options.border.style.radius, 0.0);
    let options = parse_shortcut_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--active-border-radius",
        "0",
    ]))
    .expect("shortcut-proof");
    assert_eq!(options.border.style.radius, 0.0);
    // Explicit zero round-trips through argv consistency.
    let raw = strings(&["--allowlist", "a.json", "--active-border-radius", "0"]);
    let parsed = parse_tile_proof_args(&raw).expect("parsed");
    verify_proof_argv_consistency(&raw, &parsed).expect("consistent");
}

#[test]
fn border_theme_opt_in_enables_accent_on_all_commands() {
    let tile = parse_tile_args(&strings(&["--user-start", "--active-border-theme"])).expect("tile");
    assert!(tile.border.style.use_theme);
    let proof = parse_tile_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--active-border-theme",
    ]))
    .expect("tile-proof");
    assert!(proof.border.style.use_theme);
    let shortcut = parse_shortcut_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--active-border-theme",
    ]))
    .expect("shortcut-proof");
    assert!(shortcut.border.style.use_theme);
    let workspace = parse_workspace_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--active-border-theme",
    ]))
    .expect("workspace-proof");
    assert!(workspace.border.style.use_theme);
    // Opt-in round-trips through argv consistency on every proof command.
    let raw = strings(&["--allowlist", "a.json", "--active-border-theme"]);
    let parsed = parse_tile_proof_args(&raw).expect("parsed");
    verify_proof_argv_consistency(&raw, &parsed).expect("consistent");
    let parsed = parse_shortcut_proof_args(&raw).expect("parsed");
    verify_shortcut_proof_argv_consistency(&raw, &parsed).expect("consistent");
    let parsed = parse_workspace_proof_args(&raw).expect("parsed");
    verify_workspace_proof_argv_consistency(&raw, &parsed).expect("consistent");
}

#[test]
fn border_theme_flags_conflict_refuses() {
    assert!(
        parse_tile_args(&strings(&[
            "--user-start",
            "--active-border-theme",
            "--no-active-border-theme"
        ]))
        .is_err()
    );
    assert!(
        parse_tile_proof_args(&strings(&[
            "--allowlist",
            "a.json",
            "--no-active-border-theme",
            "--active-border-theme"
        ]))
        .is_err()
    );
}

#[test]
fn border_flags_parse_on_all_commands() {
    let flags = [
        "--active-border-width",
        "4.5",
        "--active-border-gap",
        "2",
        "--active-border-radius",
        "6",
        "--active-border-color",
        "#ff0000",
        "--no-active-border-theme",
    ];
    let mut tile = vec!["--user-start"];
    tile.extend(flags);
    let options = parse_tile_args(&strings(&tile)).expect("tile");
    assert_eq!(options.border.style.width, 4.5);
    assert_eq!(options.border.style.gap, 2.0);
    assert_eq!(options.border.style.radius, 6.0);
    assert_eq!(options.border.style.color, (0xff, 0x00, 0x00));
    assert!(!options.border.style.use_theme);

    let mut proof = vec!["--allowlist", "a.json"];
    proof.extend(flags);
    let options = parse_tile_proof_args(&strings(&proof)).expect("tile-proof");
    assert_eq!(options.border.style.width, 4.5);
    assert!(!options.border.style.use_theme);

    let options = parse_shortcut_proof_args(&strings(&proof)).expect("shortcut-proof");
    assert_eq!(options.border.style.color, (0xff, 0x00, 0x00));

    let options = parse_workspace_proof_args(&strings(&proof)).expect("workspace-proof");
    assert_eq!(options.border.style.radius, 6.0);
}

#[test]
fn border_off_flag_disables() {
    let options = parse_tile_args(&strings(&["--user-start", "--no-active-border"])).expect("tile");
    assert!(!options.border.enabled);
    let options = parse_tile_proof_args(&strings(&["--allowlist", "a.json", "--no-active-border"]))
        .expect("proof");
    assert!(!options.border.enabled);
}

#[test]
fn border_validation_refuses() {
    for flags in [
        vec!["--active-border-width", "32.1"],
        vec!["--active-border-width", "-1"],
        vec!["--active-border-gap", "65"],
        vec!["--active-border-radius", "65"],
        vec!["--active-border-color", "ff0000"],
        vec!["--active-border-color", "#fff"],
        vec!["--active-border-color", "#402a82da"],
        vec!["--active-border-width"],
    ] {
        let mut args = vec!["--user-start"];
        args.extend(flags);
        assert!(parse_tile_args(&strings(&args)).is_err(), "{args:?}");
    }
}

#[test]
fn border_argv_consistency_covers_flags() {
    let raw = strings(&[
        "--allowlist",
        "a.json",
        "--trace",
        "--active-border-width",
        "4",
        "--no-active-border-theme",
    ]);
    let parsed = parse_tile_proof_args(&raw).expect("parsed");
    verify_proof_argv_consistency(&raw, &parsed).expect("consistent");

    let raw = strings(&[
        "--allowlist",
        "a.json",
        "--no-active-border",
        "--active-border-color",
        "#00ff00",
    ]);
    let parsed = parse_shortcut_proof_args(&raw).expect("parsed");
    verify_shortcut_proof_argv_consistency(&raw, &parsed).expect("consistent");

    let parsed = parse_workspace_proof_args(&raw).expect("parsed");
    verify_workspace_proof_argv_consistency(&raw, &parsed).expect("consistent");
}
