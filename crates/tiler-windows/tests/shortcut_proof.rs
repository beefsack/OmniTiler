#![forbid(unsafe_code)]
use tiler_windows::snapkey::{
    KeyboardConfig, SHORTCUT_PROOF_MARKER, SnapClassify, VK_LWIN, accept_proof_injected,
};
use tiler_windows::tiling::{
    parse_shortcut_proof_args, parse_tile_args, parse_tile_proof_args, parse_workspace_proof_args,
    verify_shortcut_proof_argv_consistency, verify_workspace_proof_argv_consistency,
};

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn marker_is_fixed_nonzero() {
    assert_eq!(SHORTCUT_PROOF_MARKER, 0x544C52_50524F4F46);
    assert_ne!(SHORTCUT_PROOF_MARKER, 0);
}

#[test]
fn product_never_accepts_injected() {
    // Product installs with no marker: every injected event passes through.
    assert!(!accept_proof_injected(0, None));
    assert!(!accept_proof_injected(SHORTCUT_PROOF_MARKER, None));
    assert!(!accept_proof_injected(0x1234, None));
}

#[test]
fn proof_accepts_only_exact_marker() {
    let proof = Some(SHORTCUT_PROOF_MARKER);
    assert!(accept_proof_injected(SHORTCUT_PROOF_MARKER, proof));
    assert!(!accept_proof_injected(0, proof));
    assert!(!accept_proof_injected(SHORTCUT_PROOF_MARKER + 1, proof));
    assert!(!accept_proof_injected(0x1234, proof));
    // A wrong configured marker never accepts, even for its own value:
    // only the fixed constant authorizes.
    assert!(!accept_proof_injected(0x1234, Some(0x1234)));
}

#[test]
fn shortcut_proof_requires_allowlist_never_normal() {
    assert!(parse_shortcut_proof_args(&strings(&[])).is_err());
    assert!(parse_shortcut_proof_args(&strings(&["--trace"])).is_err());
    assert!(parse_shortcut_proof_args(&strings(&["--seconds", "30"])).is_err());
    let ok = parse_shortcut_proof_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert_eq!(ok.allowlist.to_str().expect("path"), "a.json");
    assert!(!ok.trace);
    assert!(!ok.no_mouse_snap_prevention);
    assert_eq!(ok.seconds, None);
}

#[test]
fn shortcut_proof_parses_full_flags() {
    let ok = parse_shortcut_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--seconds",
        "120",
        "--trace",
        "--no-mouse-snap-prevention",
    ]))
    .expect("parsed");
    assert_eq!(ok.seconds, Some(120));
    assert!(ok.trace);
    assert!(ok.no_mouse_snap_prevention);
}

#[test]
fn shortcut_proof_rejects_keyboard_and_user_flags() {
    // No Win+L opt-in exists on this command: live runs can never enable it.
    for extra in [
        "--allow-win-l",
        "--no-keyboard-snap-takeover",
        "--user-start",
        "--allowlist",
    ] {
        if extra == "--allowlist" {
            continue;
        }
        assert!(
            parse_shortcut_proof_args(&strings(&["--allowlist", "a.json", extra])).is_err(),
            "{extra} must refuse"
        );
    }
    assert!(parse_shortcut_proof_args(&strings(&["--allowlist", "a.json", "--bogus"])).is_err());
    assert!(parse_shortcut_proof_args(&strings(&["--allowlist", ""])).is_err());
    assert!(
        parse_shortcut_proof_args(&strings(&["--allowlist", "a.json", "--seconds", "0"])).is_err()
    );
    assert!(
        parse_shortcut_proof_args(&strings(&["--allowlist", "a.json", "--seconds", "601"]))
            .is_err()
    );
}

#[test]
fn shortcut_proof_argv_consistency_roundtrips() {
    let parsed = parse_shortcut_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--seconds",
        "60",
        "--trace",
    ]))
    .expect("parsed");
    verify_shortcut_proof_argv_consistency(
        &strings(&["--allowlist", "a.json", "--seconds", "60", "--trace"]),
        &parsed,
    )
    .expect("consistent");
    // Missing allowlist in raw argv refuses even when parsed has one.
    assert!(
        verify_shortcut_proof_argv_consistency(&strings(&["--seconds", "60"]), &parsed).is_err()
    );
    // Trace mismatch refuses.
    assert!(
        verify_shortcut_proof_argv_consistency(
            &strings(&["--allowlist", "a.json", "--seconds", "60"]),
            &parsed
        )
        .is_err()
    );
    // Mouse flag mismatch refuses.
    let mouse = parse_shortcut_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--no-mouse-snap-prevention",
    ]))
    .expect("parsed");
    assert!(
        verify_shortcut_proof_argv_consistency(&strings(&["--allowlist", "a.json"]), &mouse)
            .is_err()
    );
    verify_shortcut_proof_argv_consistency(
        &strings(&["--allowlist", "a.json", "--no-mouse-snap-prevention"]),
        &mouse,
    )
    .expect("consistent");
}

#[test]
fn unshifted_win_l_stays_gated_without_live_lock() {
    // Portable assertion only: the shortcut-proof keyboard policy is
    // takeover-on with Win+L gated off, and the classifier passes unshifted
    // Win+L through untracked. No live Win+L is ever sent by the harness.
    let mut machine = SnapClassify::new(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    machine.push(VK_LWIN, false, true, false);
    assert_eq!(machine.push(0x4C, false, true, false), None);
    assert_eq!(machine.push(0x4C, true, true, false), None);
}

#[test]
fn workspace_proof_requires_allowlist_never_normal() {
    // Separate command from shortcut-proof: same shape, own argv contract,
    // never a silent normal run and never widened product input acceptance.
    assert!(parse_workspace_proof_args(&strings(&[])).is_err());
    assert!(parse_workspace_proof_args(&strings(&["--trace"])).is_err());
    assert!(
        parse_workspace_proof_args(&strings(&["--allowlist", "a.json", "--allow-win-l"])).is_err()
    );
    assert!(
        parse_workspace_proof_args(&strings(&["--allowlist", "a.json", "--user-start"])).is_err()
    );
    let ok = parse_workspace_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--seconds",
        "60",
        "--trace",
        "--no-mouse-snap-prevention",
    ]))
    .expect("parsed");
    assert_eq!(ok.allowlist.to_str().expect("path"), "a.json");
    assert_eq!(ok.seconds, Some(60));
    assert!(ok.trace && ok.no_mouse_snap_prevention);
    let raw = strings(&[
        "--allowlist",
        "a.json",
        "--seconds",
        "60",
        "--trace",
        "--no-mouse-snap-prevention",
    ]);
    assert!(verify_workspace_proof_argv_consistency(&raw, &ok).is_ok());
    assert!(
        verify_workspace_proof_argv_consistency(&strings(&["--allowlist", "b.json"]), &ok).is_err()
    );
}

#[test]
fn sibling_parsers_unchanged_no_fallback() {
    // Product tile still needs --user-start and rejects allowlists; proof
    // still needs its own allowlist and takes no shortcut flags.
    assert!(parse_tile_args(&strings(&["--user-start"])).is_ok());
    assert!(parse_tile_args(&strings(&[])).is_err());
    assert!(parse_tile_proof_args(&strings(&["--allowlist", "a.json"])).is_ok());
    assert!(
        parse_tile_proof_args(&strings(&[
            "--allowlist",
            "a.json",
            "--no-mouse-snap-prevention"
        ]))
        .is_err()
    );
    assert!(parse_tile_proof_args(&strings(&["--allowlist", "a.json", "--allow-win-l"])).is_err());
}
