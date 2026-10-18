#![cfg(test)]
//! Mechanical split of `cli.rs` cfg tests (no behavior change).
use crate::cli::*;
use crate::config::{DEFAULT_PROBE_SECONDS, ProbeModifier};
use crate::tile::{DEFAULT_TILE_SECONDS, TileInput};

fn argv(words: &[&str]) -> Vec<String> {
    std::iter::once("tiler-macos".to_owned())
        .chain(words.iter().map(|s| s.to_string()))
        .collect()
}

#[test]
fn default_is_nonmutating_preflight() {
    let r = parse_args(&argv(&[])).unwrap();
    assert_eq!(r.command, Command::Preflight);
    assert!(!r.may_mutate);
}

#[test]
fn help_and_check_never_mutate() {
    for cmd in [
        "help",
        "check",
        "preflight",
        "secure-input",
        "spaces-diagnose",
        "mc-inspect",
    ] {
        let r = parse_args(&argv(&[cmd])).unwrap();
        assert!(!r.may_mutate, "{cmd} must not mutate");
    }
    let r = parse_args(&argv(&["ax-enumerate"])).unwrap();
    assert!(!r.may_mutate);
}

#[test]
fn ctrl_defaults_to_ctrl() {
    let r = parse_args(&argv(&["preflight"])).unwrap();
    assert_eq!(r.modifier, ProbeModifier::Ctrl);
    let r = parse_args(&argv(&["--ctrl", "meta", "preflight"])).unwrap();
    assert_eq!(r.modifier, ProbeModifier::Meta);
}

#[test]
fn owned_modes_require_identity() {
    assert!(parse_args(&argv(&["ax-set-frame", "--pid", "9"])).is_err());
    assert!(parse_args(&argv(&["ax-observe", "--pid", "9"])).is_err());
    assert!(
        parse_args(&argv(&[
            "ax-observe",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
        ]))
        .unwrap()
        .may_mutate
    );
    let r = parse_args(&argv(&[
        "ax-set-frame",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--wid",
        "3",
    ]))
    .unwrap();
    assert!(r.may_mutate);
}

#[test]
fn input_session_needs_an_explicit_route() {
    // Explicit route is mandatory (never a default); tap is the sole route.
    // Post-route defaults to session; hid is the explicit upstream
    // discriminator.
    let r = parse_args(&argv(&["input-session", "--route", "tap"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::InputSession {
            route: crate::input_session::InputSessionRoute::Tap,
            stall_once_ms: 0,
            ..
        }
    ));
    assert!(matches!(
        r.command,
        Command::InputSession {
            post_route: crate::input_session::PostRoute::Session,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--post-route",
        "hid",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            route: crate::input_session::InputSessionRoute::Tap,
            post_route: crate::input_session::PostRoute::Hid,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--stall-once-ms",
        "1200",
        "--seconds",
        "60",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            route: crate::input_session::InputSessionRoute::Tap,
            stall_once_ms: 1200,
            seconds: 60,
            ..
        }
    ));
    // Fixture-only DIRECT hold and watchdog-only flags parse through.
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--direct-disable-hold-ms",
        "300",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            direct_disable_hold_ms: 300,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--direct-rearm",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            direct_rearm: true,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--disable-once",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            disable_once: true,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--stall-once-ms",
        "1500",
        "--physical-stall",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            stall_once_ms: 1500,
            physical_stall: true,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "input-session",
        "--route",
        "tap",
        "--seconds",
        "4",
        "--watchdog-only",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::InputSession {
            watchdog_only: true,
            seconds: 4,
            ..
        }
    ));
    assert!(parse_args(&argv(&["input-session"])).is_err());
    assert!(parse_args(&argv(&["input-session", "--route", "carbon"])).is_err());
    assert!(parse_args(&argv(&["input-session", "--route", "auto"])).is_err());
    assert!(parse_args(&argv(&["input-session", "--route", "tap", "--bogus"])).is_err());
    assert!(
        parse_args(&argv(&[
            "input-session",
            "--route",
            "tap",
            "--post-route",
            "auto",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "input-session",
            "--route",
            "tap",
            "--watchdog-only",
            "--bogus"
        ]))
        .is_err()
    );
}

#[test]
fn input_tap_suppress_is_explicit_opt_in() {
    let r = parse_args(&argv(&["input-tap"])).unwrap();
    assert!(matches!(
        r.command,
        Command::InputTap {
            suppress: false,
            ..
        }
    ));
    let r = parse_args(&argv(&["input-tap", "--suppress"])).unwrap();
    assert!(matches!(
        r.command,
        Command::InputTap { suppress: true, .. }
    ));
    // Unknown commands refuse (the removed Carbon register probe stays gone).
    assert!(parse_args(&argv(&["input-register"])).is_err());
}

#[test]
fn spaces_diagnose_stays_read_only_with_optional_wid() {
    let r = parse_args(&argv(&["spaces-diagnose"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDiagnose { wid: None, .. }
    ));
    let r = parse_args(&argv(&["spaces-diagnose", "--wid", "7"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDiagnose { wid: Some(7), .. }
    ));
    // Owned membership read stays read-only too.
    let r = parse_args(&argv(&[
        "spaces-diagnose",
        "--wid",
        "7",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
    ]))
    .unwrap();
    assert!(!r.may_mutate);
    assert!(parse_args(&argv(&["spaces-diagnose", "--nonce", "0123456789abcdef"])).is_err());
}

#[test]
fn cg_owned_windows_is_read_only_with_identity() {
    let r = parse_args(&argv(&[
        "cg-owned-windows",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
    ]))
    .unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(r.command, Command::CgOwnedWindows { .. }));
    assert!(parse_args(&argv(&["cg-owned-windows", "--pid", "9"])).is_err());
}

#[test]
fn spaces_session_is_bounded_holder_with_explicit_opt_in() {
    let r = parse_args(&argv(&["spaces-session"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesSession {
            seconds: DEFAULT_PROBE_SECONDS,
            allow_untested: false,
        }
    ));
    let r = parse_args(&argv(&[
        "spaces-session",
        "--seconds",
        "20",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::SpacesSession {
            seconds: 20,
            allow_untested: true,
        }
    ));
    assert!(parse_args(&argv(&["spaces-session", "--bogus"])).is_err());
}

#[test]
fn border_session_is_bounded_with_optional_regular_policy() {
    use crate::border_session::DEFAULT_BORDER_SECONDS;
    let r = parse_args(&argv(&["border-session"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::BorderSession {
            seconds: DEFAULT_BORDER_SECONDS,
            regular_policy: false,
        }
    ));
    let r = parse_args(&argv(&[
        "border-session",
        "--seconds",
        "30",
        "--regular-policy",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::BorderSession {
            seconds: 30,
            regular_policy: true,
        }
    ));
    assert!(parse_args(&argv(&["border-session", "--bogus"])).is_err());
    assert!(parse_args(&argv(&["border-session", "--seconds"])).is_err());
}

#[test]
fn group_underlay_session_is_bounded() {
    use crate::group_underlay_session::DEFAULT_UNDERLAY_SECONDS;
    let r = parse_args(&argv(&["group-underlay-session"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::GroupUnderlaySession {
            seconds: DEFAULT_UNDERLAY_SECONDS,
        }
    ));
    let r = parse_args(&argv(&["group-underlay-session", "--seconds", "60"])).unwrap();
    assert!(matches!(
        r.command,
        Command::GroupUnderlaySession { seconds: 60 }
    ));
    assert!(parse_args(&argv(&["group-underlay-session", "--bogus"])).is_err());
    assert!(parse_args(&argv(&["group-underlay-session", "--seconds"])).is_err());
}

#[test]
fn owned_controls_parse_with_exact_identity() {
    // owned-frame is read-only; minimize/post/focus mutate.
    let r = parse_args(&argv(&[
        "owned-frame",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--wid",
        "3",
    ]))
    .unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(r.command, Command::OwnedFrame { .. }));
    for args in [
        vec![
            "owned-minimize",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            "--minimize",
        ],
        vec![
            "owned-focus",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
        ],
        vec![
            "owned-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--chord",
            "focus-h",
        ],
        vec![
            "owned-drag-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--edge",
            "down",
            "--x",
            "300",
            "--y",
            "250",
        ],
        vec![
            "owned-drag-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--edge",
            "escape",
            "--x",
            "300",
            "--y",
            "250",
        ],
        vec!["owned-cursor"],
        vec!["owned-cursor", "--x", "300", "--y", "250"],
    ] {
        let r = parse_args(&argv(&args)).unwrap();
        assert!(r.may_mutate, "{args:?} is write-intent");
    }
    // Minimize needs exactly one of --minimize/--restore.
    assert!(
        parse_args(&argv(&[
            "owned-minimize",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "owned-minimize",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            "--minimize",
            "--restore",
        ]))
        .is_err()
    );
    // Only semantic chords parse; arbitrary keys refuse.
    for bad in [
        "ctrl-h",
        "select-0",
        "select-10",
        "workspace-select-1",
        "send-0",
        "send-10",
        "workspace-send-1",
    ] {
        assert!(
            parse_args(&argv(&[
                "owned-post",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
                "--chord",
                bad,
            ]))
            .is_err(),
            "{bad}"
        );
    }
    // Armed numbered selects parse.
    let r = parse_args(&argv(&[
        "owned-post",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--chord",
        "select-2",
    ]))
    .unwrap();
    assert!(matches!(r.command, Command::OwnedPost { .. }));
    // Armed numbered sends parse (Ctrl+Shift+digit test transport).
    let r = parse_args(&argv(&[
        "owned-post",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--chord",
        "send-2",
    ]))
    .unwrap();
    assert!(matches!(r.command, Command::OwnedPost { .. }));
    // Only lone drag edges parse; click pairs and unknown edges refuse,
    // and the point plus identity are mandatory (escape still names the
    // point for the oracle join).
    for args in [
        vec![
            "owned-drag-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--edge",
            "click",
            "--x",
            "300",
            "--y",
            "250",
        ],
        vec![
            "owned-drag-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--edge",
            "down",
            "--y",
            "250",
        ],
        vec![
            "owned-drag-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--edge",
            "down",
            "--x",
            "nan-point",
            "--y",
            "250",
        ],
        vec![
            "owned-drag-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
        ],
        vec!["owned-cursor", "--x", "300"],
        vec!["owned-cursor", "--x", "300", "--y", "bogus"],
    ] {
        assert!(parse_args(&argv(&args)).is_err(), "{args:?}");
    }
    // The --plain arm drops the Meta chord (default holds Control).
    let r = parse_args(&argv(&[
        "owned-drag-post",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--edge",
        "up",
        "--x",
        "300",
        "--y",
        "250",
        "--plain",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::OwnedDragPost { control: false, .. }
    ));
    let r = parse_args(&argv(&[
        "owned-drag-post",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--edge",
        "up",
        "--x",
        "300",
        "--y",
        "250",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::OwnedDragPost { control: true, .. }
    ));
    assert!(
        parse_args(&argv(&[
            "owned-post",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
        ]))
        .is_err()
    );
    // Missing identity refuses everywhere.
    assert!(parse_args(&argv(&["owned-frame", "--pid", "9"])).is_err());
    assert!(parse_args(&argv(&["owned-focus", "--pid", "9"])).is_err());
}

#[test]
fn textedit_post_parses_with_record_identity_and_owned_chords() {
    // Full identity parses and is write-intent (it posts synthetic input).
    let r = parse_args(&argv(&[
        "textedit-post",
        "--record",
        "/tmp/run-abc123/record.json",
        "--nonce",
        "0123456789abcdef",
        "--chord",
        "send-2",
    ]))
    .unwrap();
    assert!(
        r.may_mutate,
        "textedit-post posts input and must be write-intent"
    );
    assert!(matches!(r.command, Command::TextEditPost { .. }));
    // The product tap test transport parses: send-1..9 and select-1..9.
    for chord in [
        "send-1", "send-2", "select-1", "select-2", "focus-h", "float-g", "escape",
    ] {
        let r = parse_args(&argv(&[
            "textedit-post",
            "--record",
            "/tmp/run-abc123/record.json",
            "--nonce",
            "0123456789abcdef",
            "--chord",
            chord,
        ]))
        .unwrap();
        assert!(matches!(r.command, Command::TextEditPost { .. }), "{chord}");
        assert!(r.may_mutate, "{chord} is write-intent");
    }
    // Unarmed and unknown chords never parse (no new arm is introduced:
    // `escape` is the single diagnostic-only exception above; everything
    // else still refuses).
    for bad in [
        "select-0",
        "send-0",
        "focus-up",
        "workspace-select-1",
        "ctrl-h",
        "a",
        "space",
        "esc",
        "Escape",
        "",
    ] {
        assert!(
            parse_args(&argv(&[
                "textedit-post",
                "--record",
                "/tmp/run-abc123/record.json",
                "--nonce",
                "0123456789abcdef",
                "--chord",
                bad,
            ]))
            .is_err(),
            "{bad}"
        );
    }
    // Missing identity refuses; unknown flags refuse.
    assert!(
        parse_args(&argv(&[
            "textedit-post",
            "--nonce",
            "0123456789abcdef",
            "--chord",
            "send-2",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "textedit-post",
            "--record",
            "/tmp/run-abc123/record.json",
            "--chord",
            "send-2",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "textedit-post",
            "--record",
            "/tmp/run-abc123/record.json",
            "--nonce",
            "0123456789abcdef",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "textedit-post",
            "--record",
            "/tmp/run-abc123/record.json",
            "--nonce",
            "0123456789abcdef",
            "--chord",
            "send-2",
            "--bogus",
        ]))
        .is_err()
    );
}

#[test]
fn textedit_grab_inspect_parses_record_identity_and_never_mutates() {
    // Same record/nonce identity as the posting route but read-only: no
    // chord vocabulary, no posts on any path.
    let r = parse_args(&argv(&[
        "textedit-grab-inspect",
        "--record",
        "/tmp/run-abc123/record.json",
        "--nonce",
        "0123456789abcdef",
    ]))
    .unwrap();
    assert!(!r.may_mutate, "textedit-grab-inspect is read-only");
    assert!(matches!(r.command, Command::TextEditGrabInspect { .. }));
    // Missing identity refuses; unknown flags refuse; chord flags refuse
    // (no chord vocabulary on this route).
    assert!(
        parse_args(&argv(&[
            "textedit-grab-inspect",
            "--nonce",
            "0123456789abcdef",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "textedit-grab-inspect",
            "--record",
            "/tmp/run-abc123/record.json",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "textedit-grab-inspect",
            "--record",
            "/tmp/run-abc123/record.json",
            "--nonce",
            "0123456789abcdef",
            "--chord",
            "send-2",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "textedit-grab-inspect",
            "--record",
            "/tmp/run-abc123/record.json",
            "--nonce",
            "0123456789abcdef",
            "--bogus",
        ]))
        .is_err()
    );
}

#[test]
fn probe_spawn_close_window_is_opt_in_ms() {
    let r = parse_args(&argv(&["probe-spawn", "--close-window-ms", "9000"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            close_window_ms: 9000,
            ..
        }
    ));
    assert!(parse_args(&argv(&["probe-spawn", "--close-window-ms"])).is_err());
}

#[test]
fn probe_spawn_ls_carries_helper_argv_without_regular_policy_choice() {
    // Same owned-helper argv as probe-spawn, minus --regular-policy (which
    // is inherent to Helper.app and refuses explicitly here).
    let r = parse_args(&argv(&[
        "probe-spawn-ls",
        "--seconds",
        "90",
        "--fixed-size",
        "--close-window-ms",
        "9000",
        "--space-join",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::ProbeSpawnLs {
            seconds: 90,
            fixed_size: true,
            close_window_ms: 9000,
            space_join: true,
            ..
        }
    ));
    assert!(parse_args(&argv(&["probe-spawn-ls", "--regular-policy"])).is_err());
    assert!(parse_args(&argv(&["probe-spawn-ls", "--bogus"])).is_err());
}

#[test]
fn ax_session_is_bounded_holder_with_seconds_cap() {
    let r = parse_args(&argv(&["ax-session"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::AxSession {
            seconds: DEFAULT_PROBE_SECONDS,
        }
    ));
    let r = parse_args(&argv(&["ax-session", "--seconds", "20"])).unwrap();
    assert!(matches!(r.command, Command::AxSession { seconds: 20 }));
    assert!(parse_args(&argv(&["ax-session", "--bogus"])).is_err());
}

#[test]
fn focus_session_is_bounded_holder() {
    let r = parse_args(&argv(&["focus-session"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::FocusSession {
            seconds: DEFAULT_PROBE_SECONDS,
            appkit: false,
        }
    ));
    let r = parse_args(&argv(&["focus-session", "--seconds", "20"])).unwrap();
    assert!(matches!(
        r.command,
        Command::FocusSession {
            seconds: 20,
            appkit: false,
        }
    ));
    // AppKit lifecycle is an explicit diagnostic opt-in, never implicit.
    let r = parse_args(&argv(&["focus-session", "--appkit"])).unwrap();
    assert!(matches!(
        r.command,
        Command::FocusSession { appkit: true, .. }
    ));
    assert!(parse_args(&argv(&["focus-session", "--bogus"])).is_err());
}

#[test]
fn focus_diagnose_is_read_only_with_opt_in_flags() {
    let r = parse_args(&argv(&["focus-diagnose"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(
        r.command,
        Command::FocusDiagnose {
            fallback: false,
            appkit: false,
        }
    ));
    // Experimental fallback evidence and the AppKit init lifecycle are
    // both explicit opt-ins; either stays read-only.
    let r = parse_args(&argv(&["focus-diagnose", "--fallback"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(
        r.command,
        Command::FocusDiagnose {
            fallback: true,
            appkit: false,
        }
    ));
    let r = parse_args(&argv(&["focus-diagnose", "--appkit"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(
        r.command,
        Command::FocusDiagnose {
            fallback: false,
            appkit: true,
        }
    ));
    let r = parse_args(&argv(&["focus-diagnose", "--fallback", "--appkit"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(
        r.command,
        Command::FocusDiagnose {
            fallback: true,
            appkit: true,
        }
    ));
    assert!(parse_args(&argv(&["focus-diagnose", "--bogus"])).is_err());
}

#[test]
fn probe_spawn_fixed_size_is_explicit_opt_in() {
    let r = parse_args(&argv(&["probe-spawn"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            fixed_size: false,
            ..
        }
    ));
    let r = parse_args(&argv(&["probe-spawn", "--fixed-size"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            fixed_size: true,
            ..
        }
    ));
}

#[test]
fn probe_spawn_regular_policy_is_explicit_opt_in() {
    let r = parse_args(&argv(&["probe-spawn"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            regular_policy: false,
            ..
        }
    ));
    let r = parse_args(&argv(&["probe-spawn", "--regular-policy"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            regular_policy: true,
            ..
        }
    ));
    assert!(parse_args(&argv(&["probe-spawn", "--bogus"])).is_err());
}

#[test]
fn probe_spawn_transient_fixtures_are_opt_in_ms() {
    let r = parse_args(&argv(&["probe-spawn"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            transient_panel_ms: 0,
            transient_sheet_ms: 0,
            ..
        }
    ));
    let r = parse_args(&argv(&[
        "probe-spawn",
        "--transient-panel-ms",
        "4000",
        "--transient-sheet-ms",
        "5000",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            transient_panel_ms: 4000,
            transient_sheet_ms: 5000,
            ..
        }
    ));
    assert!(parse_args(&argv(&["probe-spawn", "--transient-panel-ms"])).is_err());
}

#[test]
fn probe_spawn_space_join_is_explicit_opt_in() {
    let r = parse_args(&argv(&["probe-spawn"])).unwrap();
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            space_join: false,
            ..
        }
    ));
    let r = parse_args(&argv(&["probe-spawn", "--space-join"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::ProbeSpawn {
            space_join: true,
            ..
        }
    ));
}

#[test]
fn owned_space_join_parses_without_wid_and_refuses_foreign_shape() {
    // No --wid by shape: the helper acts solely on its own window.
    let r = parse_args(&argv(&[
        "owned-space-join",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(r.command, Command::OwnedSpaceJoin { pid: 9, .. }));
    // Missing identity refuses; a --wid arm is not part of this command
    // (foreign-window selection is refused by shape, never parsed).
    assert!(parse_args(&argv(&["owned-space-join", "--pid", "9"])).is_err());
    assert!(
        parse_args(&argv(&[
            "owned-space-join",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
        ]))
        .is_err()
    );
}

#[test]
fn spaces_owned_recovery_parses_existing_sids_with_opt_in() {
    // Both existing SIDs mandatory; budget defaults to the helper cap;
    // explicit --allow-untested rides along for the runner gate.
    let r = parse_args(&argv(&[
        "spaces-owned-recovery",
        "--original",
        "1",
        "--temp",
        "66",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesOwnedRecovery {
            original: 1,
            temp: 66,
            seconds: crate::config::MAX_PROBE_SECONDS,
            allow_untested: true,
            probe_direct: false,
        }
    ));
    let r = parse_args(&argv(&[
        "spaces-owned-recovery",
        "--original",
        "1",
        "--temp",
        "66",
        "--seconds",
        "90",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::SpacesOwnedRecovery { seconds: 90, .. }
    ));
    // Missing SIDs refuse; unknown flags refuse.
    assert!(parse_args(&argv(&["spaces-owned-recovery", "--original", "1"])).is_err());
    assert!(parse_args(&argv(&["spaces-owned-recovery", "--temp", "66"])).is_err());
    assert!(
        parse_args(&argv(&[
            "spaces-owned-recovery",
            "--original",
            "1",
            "--temp",
            "66",
            "--bogus",
        ]))
        .is_err()
    );
}

#[test]
fn spaces_owned_recovery_probe_direct_is_opt_in_only() {
    // Default absent: the public O->T->O fixture is unchanged.
    let r = parse_args(&argv(&[
        "spaces-owned-recovery",
        "--original",
        "1",
        "--temp",
        "66",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::SpacesOwnedRecovery {
            probe_direct: false,
            ..
        }
    ));
    // Explicit opt-in rides along; the runner gate additionally requires
    // --allow-untested for the direct leg.
    let r = parse_args(&argv(&[
        "spaces-owned-recovery",
        "--original",
        "1",
        "--temp",
        "66",
        "--allow-untested",
        "--probe-direct",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesOwnedRecovery {
            original: 1,
            temp: 66,
            probe_direct: true,
            allow_untested: true,
            ..
        }
    ));
}

#[test]
fn spaces_bindings_is_read_only_and_arg_free() {
    // Read-only effective-table query: no SIDs, no opt-in, never mutating.
    let r = parse_args(&argv(&["spaces-bindings"])).unwrap();
    assert!(!r.may_mutate);
    assert!(matches!(r.command, Command::SpacesBindings));
    assert!(parse_args(&argv(&["spaces-bindings", "--bogus"])).is_err());
}

#[test]
fn spaces_desktop_commands_parse_with_explicit_intent() {
    // Apply defaults to 9 (numbered select/send) and accepts --count;
    // both are write-intent. Revert takes no args. Status is read-only.
    let r = parse_args(&argv(&["spaces-desktop-apply"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDesktopApply { count: 9 }
    ));
    let r = parse_args(&argv(&["spaces-desktop-apply", "--count", "4"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDesktopApply { count: 4 }
    ));
    assert!(parse_args(&argv(&["spaces-desktop-apply", "--bogus"])).is_err());
    let r = parse_args(&argv(&["spaces-desktop-revert"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(r.command, Command::SpacesDesktopRevert));
    assert!(parse_args(&argv(&["spaces-desktop-revert", "--bogus"])).is_err());
    let r = parse_args(&argv(&["spaces-desktop-status"])).unwrap();
    assert!(!r.may_mutate, "spaces-desktop-status must not mutate");
    assert!(matches!(r.command, Command::SpacesDesktopStatus));
    assert!(parse_args(&argv(&["spaces-desktop-status", "--bogus"])).is_err());
}

#[test]
fn spaces_switch_probe_parses_existing_sids_with_opt_in() {
    // Both existing SIDs mandatory (no Space is created); budget defaults
    // to the helper cap; explicit --allow-untested rides along.
    let r = parse_args(&argv(&[
        "spaces-switch-probe",
        "--original",
        "1",
        "--temp",
        "66",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesSwitchProbe {
            original: 1,
            temp: 66,
            seconds: crate::config::MAX_PROBE_SECONDS,
            allow_untested: true,
        }
    ));
    // Missing SIDs or unknown flags refuse before any live touch.
    assert!(parse_args(&argv(&["spaces-switch-probe", "--original", "1"])).is_err());
    assert!(parse_args(&argv(&["spaces-switch-probe", "--temp", "66"])).is_err());
    assert!(
        parse_args(&argv(&[
            "spaces-switch-probe",
            "--original",
            "1",
            "--temp",
            "66",
            "--bogus",
        ]))
        .is_err()
    );
}

#[test]
fn spaces_drag_transfer_parses_existing_sids_with_opt_in() {
    // Both existing SIDs mandatory (no Space is created); budget defaults
    // to the helper cap; explicit --allow-untested rides along. Mutating
    // (synthetic drag plus visible switches).
    let r = parse_args(&argv(&[
        "spaces-drag-transfer",
        "--original",
        "1",
        "--temp",
        "66",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDragTransfer {
            original: 1,
            temp: Some(66),
            seconds: crate::config::MAX_PROBE_SECONDS,
            allow_untested: true,
            numbered: false,
        }
    ));
    assert!(parse_args(&argv(&["spaces-drag-transfer", "--original", "1"])).is_err());
    assert!(parse_args(&argv(&["spaces-drag-transfer", "--temp", "66"])).is_err());
    assert!(
        parse_args(&argv(&[
            "spaces-drag-transfer",
            "--original",
            "1",
            "--temp",
            "66",
            "--bogus",
        ]))
        .is_err()
    );
}

#[test]
fn spaces_drag_transfer_temporary_parses_without_temp() {
    // --temporary replaces --temp (exactly one of the two): the runner
    // creates exactly one temp internally and removes only it. Mutating.
    let r = parse_args(&argv(&[
        "spaces-drag-transfer",
        "--original",
        "1",
        "--temporary",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDragTransfer {
            original: 1,
            temp: None,
            seconds: crate::config::MAX_PROBE_SECONDS,
            allow_untested: true,
            numbered: false,
        }
    ));
    // --temp and --temporary are exclusive; bare --original still refuses.
    assert!(
        parse_args(&argv(&[
            "spaces-drag-transfer",
            "--original",
            "1",
            "--temp",
            "66",
            "--temporary",
        ]))
        .is_err()
    );
    assert!(parse_args(&argv(&["spaces-drag-transfer", "--original", "1"])).is_err());
}

#[test]
fn spaces_drag_transfer_numbered_defaults_to_arrows() {
    // Default stays arrows; --numbered opts into the Desktop-N chord
    // variant explicitly (no settings write on any path; the runner
    // refuses unusable holders before any input).
    let r = parse_args(&argv(&[
        "spaces-drag-transfer",
        "--original",
        "277",
        "--temp",
        "1",
        "--numbered",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::SpacesDragTransfer {
            original: 277,
            temp: Some(1),
            numbered: true,
            ..
        }
    ));
    // --numbered composes with --temporary too.
    let r = parse_args(&argv(&[
        "spaces-drag-transfer",
        "--original",
        "277",
        "--temporary",
        "--numbered",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::SpacesDragTransfer {
            temp: None,
            numbered: true,
            ..
        }
    ));
    assert!(parse_args(&argv(&["spaces-drag-transfer", "--numbered"])).is_err());
}

#[test]
fn owned_fullscreen_parses_with_exact_identity_and_single_arm() {
    for (flag, want) in [("--enter", true), ("--exit", false)] {
        let r = parse_args(&argv(&[
            "owned-fullscreen",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            flag,
        ]))
        .unwrap();
        assert!(r.may_mutate, "{flag} is write-intent");
        assert!(matches!(
            r.command,
            Command::OwnedFullscreen {
                fullscreen: got,
                ..
            } if got == want
        ));
    }
    // Exactly one arm: missing or both refuse.
    assert!(
        parse_args(&argv(&[
            "owned-fullscreen",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "owned-fullscreen",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            "--enter",
            "--exit",
        ]))
        .is_err()
    );
}

#[test]
fn tile_parses_with_explicit_scope_start_and_bounded_lifetime() {
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
        "--scope",
        "com.apple.Calculator",
        "--seconds",
        "30",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::Tile {
            user_start: true,
            seconds: 30,
            input: TileInput::Tap,
            preset: crate::input_policy::InputPreset::Authentic,
            ..
        }
    ));
    if let Command::Tile { scopes, .. } = r.command {
        assert_eq!(scopes, vec!["com.apple.TextEdit", "com.apple.Calculator"]);
    }
    // Defaults: tap route, authentic preset, 30 s.
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            seconds: DEFAULT_TILE_SECONDS,
            input: TileInput::Tap,
            preset: crate::input_policy::InputPreset::Authentic,
            ..
        }
    ));
    // Explicit off route only (never auto, never carbon).
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
        "--input",
        "off",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            input: TileInput::Off,
            ..
        }
    ));
    // Explicit tap parses (the default, spelled out).
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
        "--input",
        "tap",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            input: TileInput::Tap,
            ..
        }
    ));
    // Compatible preset selects conflict passthrough.
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
        "--preset",
        "compatible",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            preset: crate::input_policy::InputPreset::Compatible,
            ..
        }
    ));
    assert!(
        parse_args(&argv(&[
            "tile",
            "--user-start",
            "--scope",
            "x",
            "--input",
            "auto"
        ]))
        .is_err()
    );
    // Carbon stays unparsed (no register probe, no carbon route).
    assert!(
        parse_args(&argv(&[
            "tile",
            "--user-start",
            "--scope",
            "x",
            "--input",
            "carbon"
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "tile",
            "--user-start",
            "--scope",
            "x",
            "--preset",
            "keep"
        ]))
        .is_err()
    );
}

#[test]
fn tile_never_implies_scope_or_start() {
    // No flags at all: parses (validation refuses later), but carries
    // no implicit scope and no start token.
    let r = parse_args(&argv(&["tile"])).unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            user_start: false,
            seconds: DEFAULT_TILE_SECONDS,
            ..
        }
    ));
    if let Command::Tile { scopes, .. } = r.command {
        assert!(scopes.is_empty());
    }
    // Unknown flags refuse at parse time.
    assert!(parse_args(&argv(&["tile", "--user-start", "--scope", "x", "--bogus"])).is_err());
    assert!(parse_args(&argv(&["tile", "--scope"])).is_err());
}

#[test]
fn tile_border_flags_default_unset_and_parse_raw() {
    // No border flags: every selection stays unset (startup merge fills
    // defaults from the settings file, else shared parity).
    let r = parse_args(&argv(&["tile", "--user-start", "--scope", "x"])).unwrap();
    if let Command::Tile { border, .. } = r.command {
        assert_eq!(border, crate::dev_selection::BorderCli::default());
    } else {
        panic!("want tile");
    }
    // Every border flag parses (values stay raw: bounds validate at
    // startup merge with the shared errors).
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--no-active-border",
        "--active-border-width",
        "5",
        "--active-border-gap",
        "2",
        "--active-border-radius",
        "10",
        "--active-border-color",
        "#112233",
        "--active-border-theme",
    ]))
    .unwrap();
    if let Command::Tile { border, .. } = r.command {
        assert_eq!(border.enabled, Some(false));
        assert_eq!(border.width.as_deref(), Some("5"));
        assert_eq!(border.gap.as_deref(), Some("2"));
        assert_eq!(border.radius.as_deref(), Some("10"));
        assert_eq!(border.color.as_deref(), Some("#112233"));
        assert_eq!(border.use_theme, Some(true));
    } else {
        panic!("want tile");
    }
    // The negative theme flag wins when last (plain last-wins, like preset).
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--active-border-theme",
        "--no-active-border-theme",
    ]))
    .unwrap();
    if let Command::Tile { border, .. } = r.command {
        assert_eq!(border.use_theme, Some(false));
    } else {
        panic!("want tile");
    }
    // Missing values refuse at parse time.
    assert!(parse_args(&argv(&["tile", "--active-border-width"])).is_err());
    assert!(parse_args(&argv(&["tile", "--active-border-color"])).is_err());
}

#[test]
fn tile_indicator_flags_default_unset_and_parse() {
    // No indicator flags: every selection stays unset (startup merge fills
    // the defaults: all rows, no icons).
    let r = parse_args(&argv(&["tile", "--user-start", "--scope", "x"])).unwrap();
    if let Command::Tile { indicator, .. } = r.command {
        assert_eq!(
            indicator,
            crate::workspace_indicator::IndicatorCli::default()
        );
    } else {
        panic!("want tile");
    }
    // Every indicator flag parses.
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--workspace-indicator",
        "current",
        "--workspace-indicator-icons",
    ]))
    .unwrap();
    if let Command::Tile { indicator, .. } = r.command {
        assert_eq!(
            indicator.mode,
            Some(crate::workspace_indicator::IndicatorMode::Current)
        );
        assert_eq!(indicator.icons, Some(true));
    } else {
        panic!("want tile");
    }
    // The negative icons flag wins when last (plain last-wins).
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--workspace-indicator-icons",
        "--no-workspace-indicator-icons",
    ]))
    .unwrap();
    if let Command::Tile { indicator, .. } = r.command {
        assert_eq!(indicator.icons, Some(false));
    } else {
        panic!("want tile");
    }
    // Off mode parses; unknown modes and missing values refuse.
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--scope",
        "x",
        "--workspace-indicator",
        "off",
    ]))
    .unwrap();
    if let Command::Tile { indicator, .. } = r.command {
        assert_eq!(
            indicator.mode,
            Some(crate::workspace_indicator::IndicatorMode::Off)
        );
    } else {
        panic!("want tile");
    }
    assert!(parse_args(&argv(&["tile", "--workspace-indicator", "numbers"])).is_err());
    assert!(parse_args(&argv(&["tile", "--workspace-indicator"])).is_err());
    assert!(parse_args(&argv(&["tile", "--workspace-indicator-icons-extra"])).is_err());
}

#[test]
fn workspace_indicator_status_is_read_only() {
    // No args: read-only dump, never mutating.
    let r = parse_args(&argv(&["workspace-indicator-status"])).unwrap();
    assert!(matches!(r.command, Command::WorkspaceIndicatorStatus));
    assert!(!r.may_mutate);
    // Any arg refuses (usage, not silent acceptance).
    assert!(parse_args(&argv(&["workspace-indicator-status", "--seconds", "5"])).is_err());
}

#[test]
fn workspace_indicator_proof_needs_no_identity_but_may_mutate() {
    // Bounded live proof over the production guard: no probe-spawn
    // identity involved (the fixture spawns its own owned helper), but
    // the command may mutate topology and posts one synthetic segment
    // click pair, so it is gated separately from the read-only status
    // dump. Defaults select display 0 with the 120 s proof lifetime;
    // --allow-untested rides the payload (the runner refuses without it).
    let r = parse_args(&argv(&["workspace-indicator-proof", "--allow-untested"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::WorkspaceIndicatorProof {
            seconds: 120,
            display: 0,
            allow_untested: true,
        }
    ));
    let r = parse_args(&argv(&[
        "workspace-indicator-proof",
        "--seconds",
        "90",
        "--display",
        "0",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::WorkspaceIndicatorProof {
            seconds: 90,
            display: 0,
            allow_untested: true,
        }
    ));
    // Without the opt-in the parse still succeeds (the runner refuses with
    // a named prerequisite); unknown flags never parse.
    let r = parse_args(&argv(&["workspace-indicator-proof"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::WorkspaceIndicatorProof {
            allow_untested: false,
            ..
        }
    ));
    assert!(parse_args(&argv(&["workspace-indicator-proof", "--bogus"])).is_err());
    assert!(parse_args(&argv(&["workspace-indicator-proof", "--seconds"])).is_err());
}

#[test]
fn tile_unscoped_flags_and_override_explicitness() {
    // Unscoped product mode with repeatable exclusions and log verbosity.
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--exclude",
        "com.example.a",
        "--exclude",
        "com.example.b",
        "--log",
        "verbose",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            user_start: true,
            unscoped: true,
            log: crate::tile::TileLogLevel::Verbose,
            preset: crate::input_policy::InputPreset::Authentic,
            ..
        }
    ));
    if let Command::Tile {
        excluded,
        preset_explicit,
        ..
    } = r.command
    {
        assert_eq!(excluded, vec!["com.example.a", "com.example.b"]);
        assert!(!preset_explicit);
    }
    assert!(!r.modifier_explicit);
    // Explicit --preset and --ctrl ride along for settings-file override.
    let r = parse_args(&argv(&[
        "--ctrl",
        "meta",
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
        "--preset",
        "compatible",
    ]))
    .unwrap();
    assert!(r.modifier_explicit);
    assert!(matches!(
        r.command,
        Command::Tile {
            preset: crate::input_policy::InputPreset::Compatible,
            preset_explicit: true,
            ..
        }
    ));
    // Defaults carry no explicitness (settings file applies).
    let r = parse_args(&argv(&["tile", "--user-start", "--scope", "x"])).unwrap();
    assert!(!r.modifier_explicit);
    assert!(matches!(
        r.command,
        Command::Tile {
            preset_explicit: false,
            unscoped: false,
            log: crate::tile::TileLogLevel::Normal,
            ..
        }
    ));
    // Exact-run directory opts into graceful lifecycle.
    let r = parse_args(&argv(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--run-dir",
        "/tmp/run-1",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::Tile {
            unscoped: true,
            run_dir: Some(_),
            ..
        }
    ));
    if let Command::Tile { run_dir, .. } = r.command {
        assert_eq!(run_dir.as_deref(), Some("/tmp/run-1"));
    }
    assert!(parse_args(&argv(&["tile", "--user-start", "--run-dir"])).is_err());
}

#[test]
fn space_write_modes_require_full_identity_and_may_mutate() {
    for args in [
        vec![
            "spaces-move",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            "--space",
            "5",
            "--allow-untested",
        ],
        vec![
            "spaces-restore",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            "--space",
            "5",
            "--allow-untested",
        ],
        vec![
            "spaces-switch",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--space",
            "5",
        ],
        vec!["spaces-create", "--pid", "9", "--nonce", "0123456789abcdef"],
        vec![
            "spaces-destroy",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--space",
            "5",
        ],
        vec![
            "spaces-owned-recovery",
            "--original",
            "1",
            "--temp",
            "66",
            "--allow-untested",
        ],
    ] {
        let r = parse_args(&argv(&args)).unwrap();
        assert!(r.may_mutate, "{args:?} is write-intent");
    }
    // The untested prerequisite defaults off (explicit opt-in only).
    let r = parse_args(&argv(&[
        "spaces-move",
        "--pid",
        "9",
        "--nonce",
        "0123456789abcdef",
        "--wid",
        "3",
        "--space",
        "5",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::SpacesMove {
            allow_untested: false,
            ..
        }
    ));
    assert!(
        parse_args(&argv(&[
            "spaces-move",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "spaces-switch",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
        ]))
        .is_err()
    );
}

#[test]
fn mc_members_is_read_only_with_mandatory_space() {
    // Read-only Space-members inspection: one mandatory --space SID, no
    // writes on any path, unknown flags never parse.
    let r = parse_args(&argv(&["mc-members", "--space", "66"])).unwrap();
    assert_eq!(r.command, Command::McMembers { space: 66 });
    assert!(!r.may_mutate, "mc-members must not mutate");
    assert!(parse_args(&argv(&["mc-members"])).is_err());
    assert!(parse_args(&argv(&["mc-members", "--space"])).is_err());
    assert!(parse_args(&argv(&["mc-members", "--space", "66", "--bogus"])).is_err());
    assert!(parse_args(&argv(&["mc-members", "--space", "nope"])).is_err());
}

#[test]
fn mc_inspect_is_read_only_without_args() {
    // Read-only Mission Control inspection: no launch, no writes, and no
    // arguments accepted (any flag is a usage error, never a default).
    let r = parse_args(&argv(&["mc-inspect"])).unwrap();
    assert_eq!(r.command, Command::McInspect);
    assert!(!r.may_mutate, "mc-inspect must not mutate");
    assert!(parse_args(&argv(&["mc-inspect", "--seconds", "10"])).is_err());
}

#[test]
fn mc_recover_needs_named_temp_and_original_but_may_mutate() {
    // Bounded recovery: the adopted temp and the preimage original are
    // both mandatory (never guessed); display/seconds default as in the
    // lifecycle; --allow-untested rides the payload (the runner refuses
    // without it — no default mutation). Never creates a Space.
    let r = parse_args(&argv(&[
        "mc-recover",
        "--temp",
        "66",
        "--original",
        "1",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::McRecover {
            temp: 66,
            original: 1,
            seconds: 60,
            display: 0,
            allow_untested: true,
        }
    ));
    let r = parse_args(&argv(&[
        "mc-recover",
        "--temp",
        "66",
        "--original",
        "1",
        "--seconds",
        "75",
        "--display",
        "1",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::McRecover {
            temp: 66,
            original: 1,
            seconds: 75,
            display: 1,
            allow_untested: true,
        }
    ));
    // Temp and original are both mandatory; unknown flags never parse.
    assert!(parse_args(&argv(&["mc-recover", "--temp", "66"])).is_err());
    assert!(parse_args(&argv(&["mc-recover", "--original", "1"])).is_err());
    assert!(parse_args(&argv(&["mc-recover"])).is_err());
    assert!(
        parse_args(&argv(&[
            "mc-recover",
            "--temp",
            "66",
            "--original",
            "1",
            "--bogus",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "mc-recover",
            "--temp",
            "66",
            "--original",
            "1",
            "--display",
        ]))
        .is_err()
    );
}

#[test]
fn mc_lifecycle_needs_no_identity_but_may_mutate() {
    // Explicit temp-Space lifecycle: no probe-spawn identity involved (no
    // window is ever targeted), but the command may mutate topology, so it
    // is gated separately from inspect. Defaults select display 0 with the
    // standard probe lifetime; --allow-untested rides the command payload
    // (the runner refuses without it — no default mutation).
    let r = parse_args(&argv(&["mc-lifecycle", "--allow-untested"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::McLifecycle {
            seconds: 60,
            display: 0,
            allow_untested: true,
        }
    ));
    let r = parse_args(&argv(&[
        "mc-lifecycle",
        "--seconds",
        "75",
        "--display",
        "1",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::McLifecycle {
            seconds: 75,
            display: 1,
            allow_untested: true,
        }
    ));
    // Without the opt-in the parse still succeeds (the runner refuses with
    // a named prerequisite); unknown flags never parse.
    let r = parse_args(&argv(&["mc-lifecycle"])).unwrap();
    assert!(matches!(
        r.command,
        Command::McLifecycle {
            allow_untested: false,
            ..
        }
    ));
    assert!(parse_args(&argv(&["mc-lifecycle", "--bogus"])).is_err());
    assert!(parse_args(&argv(&["mc-lifecycle", "--display"])).is_err());
}

#[test]
fn mc_restore_frontmost_couples_exact_target_and_may_mutate() {
    // Guarded Ghostty-only restoration: PID, WID, and Space are all
    // mandatory (never guessed); display defaults to 0. May mutate (one
    // public activation), never a general window API.
    let r = parse_args(&argv(&[
        "mc-restore-frontmost",
        "--pid",
        "42",
        "--wid",
        "7",
        "--space",
        "10",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::McRestoreFrontmost {
            pid: 42,
            wid: 7,
            space: 10,
            display: 0,
            allow_untested: true,
        }
    ));
    // Without the opt-in the parse still succeeds (the runner refuses with
    // a named prerequisite before any mutation); display defaults to 0.
    let r = parse_args(&argv(&[
        "mc-restore-frontmost",
        "--pid",
        "42",
        "--wid",
        "7",
        "--space",
        "10",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::McRestoreFrontmost {
            allow_untested: false,
            ..
        }
    ));
    // A missing half never parses; unknown flags never parse.
    assert!(parse_args(&argv(&["mc-restore-frontmost", "--pid", "42"])).is_err());
    assert!(
        parse_args(&argv(&[
            "mc-restore-frontmost",
            "--pid",
            "42",
            "--wid",
            "7",
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "mc-restore-frontmost",
            "--pid",
            "42",
            "--wid",
            "7",
            "--space",
            "10",
            "--bogus",
        ]))
        .is_err()
    );
}
#[test]
fn grab_diagnose_is_explicit_mutation_and_takes_no_arguments() {
    let args = vec!["probe".to_owned(), "spaces-grab-diagnose".to_owned()];
    let request = crate::cli::parse_args(&args).unwrap();
    assert_eq!(request.command, crate::cli::Command::SpacesGrabDiagnose);
    assert!(request.may_mutate); // Helper focus and Mission Control cycle.
    let mut extra = args;
    extra.push("--ignored".to_owned());
    assert!(crate::cli::parse_args(&extra).is_err());
}

#[test]
fn select_batch_requires_explicit_select_ids_and_rejects_send() {
    assert!(parse_args(&argv(&["spaces-select"])).is_err());
    assert!(parse_args(&argv(&["spaces-select", "--action", "workspace-send-1"])).is_err());
    let r = parse_args(&argv(&[
        "spaces-select",
        "--action",
        "workspace-select-0",
        "--action",
        "workspace-previous",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(r.command,Command::SpacesSelect {display:0,actions} if actions.len() == 2));
    let cg = parse_args(&argv(&["cg-inspect"])).unwrap();
    assert!(!cg.may_mutate);
}

#[test]
fn empty_removal_needs_explicit_nonzero_sid_and_opt_in() {
    assert!(
        parse_args(&argv(&[
            "mc-remove-empty",
            "--space",
            "0",
            "--allow-untested"
        ]))
        .is_err()
    );
    assert!(parse_args(&argv(&["mc-remove-empty", "--space", "2"])).is_err());
    let r = parse_args(&argv(&[
        "mc-remove-empty",
        "--space",
        "2",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::McRemoveEmpty {
            space: 2,
            allow_untested: true
        }
    ));
}

#[test]
fn drag_diag_chord_parses_with_countdown_default_and_opts() {
    use crate::space_drag_diag::DEFAULT_COUNTDOWN_SECS;
    let r = parse_args(&argv(&["drag-diag-chord", "--allow-untested"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::DragDiagChord {
            countdown: DEFAULT_COUNTDOWN_SECS,
            untagged: false,
            allow_untested: true,
        }
    ));
    assert_eq!(DEFAULT_COUNTDOWN_SECS, 5);
    let r = parse_args(&argv(&[
        "drag-diag-chord",
        "--countdown",
        "0",
        "--untagged",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::DragDiagChord {
            countdown: 0,
            untagged: true,
            ..
        }
    ));
    assert!(parse_args(&argv(&["drag-diag-chord", "--countdown", "31"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-chord", "--countdown"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-chord", "--stream"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-chord", "--bogus"])).is_err());
}

#[test]
fn drag_diag_hold_parses_with_bounded_hold_cap() {
    use crate::space_drag_diag::{DEFAULT_COUNTDOWN_SECS, DEFAULT_HOLD_SECS, MAX_HOLD_SECS};
    let r = parse_args(&argv(&["drag-diag-hold", "--allow-untested"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::DragDiagHold {
            countdown: DEFAULT_COUNTDOWN_SECS,
            hold_secs: DEFAULT_HOLD_SECS,
            allow_untested: true,
        }
    ));
    assert_eq!((DEFAULT_HOLD_SECS, MAX_HOLD_SECS), (6, 15));
    let r = parse_args(&argv(&[
        "drag-diag-hold",
        "--countdown",
        "3",
        "--hold-secs",
        "15",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::DragDiagHold {
            countdown: 3,
            hold_secs: 15,
            ..
        }
    ));
    // The hold cap is a parse refusal (before any live touch): 0 and 16+.
    assert!(parse_args(&argv(&["drag-diag-hold", "--hold-secs", "0"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-hold", "--hold-secs", "16"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-hold", "--hold-secs"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-hold", "--untagged"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-hold", "--stream"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-hold", "--bogus"])).is_err());
}

#[test]
fn drag_diag_full_parses_with_amethyst_variant() {
    use crate::space_drag_diag::{DEFAULT_AMETHYST_DESKTOP, DEFAULT_COUNTDOWN_SECS};
    let r = parse_args(&argv(&["drag-diag-full", "--allow-untested"])).unwrap();
    assert!(r.may_mutate);
    assert!(matches!(
        r.command,
        Command::DragDiagFull {
            countdown: DEFAULT_COUNTDOWN_SECS,
            amethyst: false,
            desktop: DEFAULT_AMETHYST_DESKTOP,
            untagged: false,
            allow_untested: true,
        }
    ));
    assert_eq!(DEFAULT_AMETHYST_DESKTOP, 2);
    let r = parse_args(&argv(&[
        "drag-diag-full",
        "--amethyst",
        "--desktop",
        "3",
        "--allow-untested",
    ]))
    .unwrap();
    assert!(matches!(
        r.command,
        Command::DragDiagFull {
            amethyst: true,
            desktop: 3,
            ..
        }
    ));
    // --desktop needs --amethyst; --untagged is redundant with it; the
    // Silica range 1..=16 is enforced at parse.
    assert!(
        parse_args(&argv(&[
            "drag-diag-full",
            "--desktop",
            "3",
            "--allow-untested"
        ]))
        .is_err()
    );
    assert!(
        parse_args(&argv(&[
            "drag-diag-full",
            "--amethyst",
            "--untagged",
            "--allow-untested"
        ]))
        .is_err()
    );
    assert!(parse_args(&argv(&["drag-diag-full", "--desktop", "0"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-full", "--desktop", "17"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-full", "--amethyst", "--desktop"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-full", "--stream"])).is_err());
    assert!(parse_args(&argv(&["drag-diag-full", "--bogus"])).is_err());
}
