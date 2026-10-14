//! Cross-module boundedness / ownership / input-matching tests.
//! Portable: run on Linux, Windows, and macOS CI without live UI/hooks.

use tiler_macos::cli::parse_args;
use tiler_macos::config::{MAX_TAP_SECONDS, ProbeConfig, ProbeModifier};
use tiler_macos::input_match::Chord;
use tiler_macos::ownership::OwnedIdentity;

fn argv(words: &[&str]) -> Vec<String> {
    std::iter::once("tiler-macos".to_owned())
        .chain(words.iter().map(|s| s.to_string()))
        .collect()
}

#[test]
fn tap_bound_is_global() {
    assert_eq!(MAX_TAP_SECONDS, 30);
    let cfg = ProbeConfig::default();
    cfg.validate().unwrap();
}

#[test]
fn owned_gate_never_covers_ghostty() {
    let id = OwnedIdentity::new(9001, tiler_macos::BUNDLE_ID, "0123456789abcdef").unwrap();
    assert!(id.check_action(9001, tiler_macos::BUNDLE_ID).is_ok());
    for foreign in [
        "com.mitchellh.ghostty",
        "com.apple.Terminal",
        "com.example.other",
    ] {
        assert!(id.check_action(9001, foreign).is_err(), "{foreign}");
    }
    assert!(id.check_action(9002, tiler_macos::BUNDLE_ID).is_err());
}

#[test]
fn ctrl_chord_exact_match() {
    let chord = Chord::new(ProbeModifier::Meta, 0x04).unwrap();
    assert!(chord.matches(ProbeModifier::Meta, 0x04));
    assert!(!chord.matches(ProbeModifier::Meta, 0x05));
    assert!(!chord.matches(ProbeModifier::Ctrl, 0x04));
}

#[test]
fn spaces_move_fails_closed_without_live_session() {
    // No spawn record exists for this PID on any platform: the full record
    // proof fails closed before any SLS write.
    let id = OwnedIdentity::new(9001, tiler_macos::BUNDLE_ID, "0123456789abcdef").unwrap();
    let err =
        tiler_macos::spaces::move_owned_window_to_space(&id, 9001, "0123456789abcdef", 3, 2, true)
            .unwrap_err();
    assert!(matches!(
        err,
        tiler_macos::spaces::SpacesError::Ownership(_)
    ));
}

#[test]
fn spaces_move_requires_explicit_untested_prerequisite() {
    // Prerequisite gate fires before any live touch.
    let id = OwnedIdentity::new(9001, tiler_macos::BUNDLE_ID, "0123456789abcdef").unwrap();
    let err =
        tiler_macos::spaces::move_owned_window_to_space(&id, 9001, "0123456789abcdef", 3, 2, false)
            .unwrap_err();
    assert!(matches!(err, tiler_macos::spaces::SpacesError::Blocked(_)));
    assert!(err.to_string().contains("--allow-untested"));
}

#[test]
fn spaces_restore_validates_identity_every_write() {
    let id = OwnedIdentity::new(9001, tiler_macos::BUNDLE_ID, "0123456789abcdef").unwrap();
    let err = tiler_macos::spaces::restore_owned_window_to_space(
        &id,
        9001,
        "0123456789abcdef",
        3,
        2,
        true,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        tiler_macos::spaces::SpacesError::Ownership(_)
    ));
}

#[test]
fn spaces_switch_create_destroy_report_unsupported_routes() {
    let switch = tiler_macos::spaces::switch_space_status(2);
    assert!(!switch.supported);
    assert!(switch.reason.contains("scripting-addition"));
    assert!(!switch.route.contains("SA-only"));
    // Non-SA alternatives named; refusal is missing recovery, not SIP.
    assert!(switch.route.contains("gotoSpace"));
    assert!(switch.reason.contains("Mission Control"));
    assert!(switch.reason.contains("not a SIP rejection"));

    let create = tiler_macos::spaces::create_space_status();
    assert!(!create.supported);
    assert!(create.reason.contains("scripting-addition"));
    // Non-SA alternative named; refusal is missing recovery, not SIP.
    assert!(create.reason.contains("Mission Control"));
    assert!(create.reason.contains("not a SIP rejection"));

    let destroy = tiler_macos::spaces::destroy_space_status(2);
    assert!(!destroy.supported);
    assert!(destroy.reason.contains("scripting-addition"));
    assert!(destroy.reason.contains("Mission Control"));
    assert!(destroy.reason.contains("not a SIP rejection"));
}

#[test]
fn cg_verify_refuses_zero_wid_before_os_touch() {
    let err = tiler_macos::cg::verify_wid_owned_by_pid(9001, 0).unwrap_err();
    assert!(matches!(err, tiler_macos::cg::CgError::Ownership(_)));
}

#[test]
fn preflight_and_help_do_not_mutate() {
    for args in [
        argv(&[]),
        argv(&["help"]),
        argv(&["preflight"]),
        argv(&["check"]),
    ] {
        let req = parse_args(&args).unwrap();
        assert!(!req.may_mutate);
    }
}
