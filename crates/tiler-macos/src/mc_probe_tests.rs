#![cfg(test)]
//! Offline policy tests for the Mission Control probe: SID diff, order,
//! removal, shape, budget, classification, emptiness, and focus gates. Pure
//! data only — no live dispatch, no windows touched.

use crate::mc_probe::*;

fn display(spaces: &[u64], current: Option<u64>) -> DisplaySpaces {
    DisplaySpaces {
        identifier: Some("uuid-test".to_owned()),
        spaces: spaces.to_vec(),
        current,
    }
}

fn snapshot(rows: Vec<DisplaySpaces>) -> TopologySnapshot {
    TopologySnapshot { displays: rows }
}

#[test]
fn locate_is_exact_and_rejects_ambiguity() {
    let s = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    assert_eq!(s.locate(12), Some((0, 1)));
    assert_eq!(s.locate(21), Some((1, 0)));
    assert_eq!(s.locate(99), None);
    let dup = snapshot(vec![display(&[7], Some(7)), display(&[7], Some(7))]);
    assert_eq!(dup.locate(7), None);
}

#[test]
fn create_effect_accepts_exactly_one_new_sid_in_order() {
    let before = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    let after = snapshot(vec![
        display(&[11, 12, 13], Some(11)),
        display(&[21], Some(21)),
    ]);
    assert_eq!(validate_create_effect(&before, &after, 0), Ok(13));
}

#[test]
fn create_effect_rejects_zero_or_two_new_sids() {
    let before = snapshot(vec![display(&[11, 12], Some(11))]);
    let same = snapshot(vec![display(&[11, 12], Some(11))]);
    let err = validate_create_effect(&before, &same, 0).unwrap_err();
    assert!(matches!(err, McError::Blocked(_)));
    assert!(err.to_string().contains("exactly one new Space"));
    let two = snapshot(vec![display(&[11, 12, 13, 14], Some(11))]);
    let err = validate_create_effect(&before, &two, 0).unwrap_err();
    assert!(err.to_string().contains("exactly one new Space"));
}

#[test]
fn create_effect_rejects_wrong_display_order_change_and_current_switch() {
    let before = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    let wrong = snapshot(vec![
        display(&[11, 12], Some(11)),
        display(&[21, 22], Some(21)),
    ]);
    let err = validate_create_effect(&before, &wrong, 0).unwrap_err();
    assert!(err.to_string().contains("not on chosen display"));
    let reordered = snapshot(vec![
        display(&[12, 11, 13], Some(11)),
        display(&[21], Some(21)),
    ]);
    let err = validate_create_effect(&before, &reordered, 0).unwrap_err();
    assert!(err.to_string().contains("order changed"));
    let switched = snapshot(vec![
        display(&[11, 12, 13], Some(13)),
        display(&[21], Some(21)),
    ]);
    let err = validate_create_effect(&before, &switched, 0).unwrap_err();
    assert!(err.to_string().contains("current changed"));
}

#[test]
fn create_effect_rejects_duplicates_and_shape_changes() {
    let before = snapshot(vec![display(&[11, 12], Some(11))]);
    // Duplicate SID in the readback: uniqueness unprovable.
    let dup = snapshot(vec![display(&[11, 12, 12], Some(11))]);
    let err = validate_create_effect(&before, &dup, 0).unwrap_err();
    assert!(err.to_string().contains("duplicate"));
    // Display count change before any zip.
    let fewer = TopologySnapshot { displays: vec![] };
    let err = validate_create_effect(&before, &fewer, 0).unwrap_err();
    assert!(err.to_string().contains("display count"));
    // Identifier change on the same row: no positional reasoning.
    let renamed = TopologySnapshot {
        displays: vec![DisplaySpaces {
            identifier: Some("other-uuid".to_owned()),
            spaces: vec![11, 12, 13],
            current: Some(11),
        }],
    };
    let err = validate_create_effect(&before, &renamed, 0).unwrap_err();
    assert!(err.to_string().contains("identity changed"));
}

#[test]
fn goto_effect_requires_sets_equal_and_exact_current() {
    let before = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    let moved = snapshot(vec![display(&[11, 12], Some(12)), display(&[21], Some(21))]);
    assert!(validate_goto_effect(&before, &moved, 0, 12).is_ok());
    let err = validate_goto_effect(&before, &before, 0, 12).unwrap_err();
    assert!(err.to_string().contains("current on display 0"));
    let drift = snapshot(vec![display(&[11, 12], Some(12)), display(&[12], Some(12))]);
    let err = validate_goto_effect(&before, &drift, 0, 12).unwrap_err();
    assert!(matches!(err, McError::Blocked(_)));
    let grown = snapshot(vec![
        display(&[11, 12, 13], Some(12)),
        display(&[21], Some(21)),
    ]);
    let err = validate_goto_effect(&before, &grown, 0, 12).unwrap_err();
    assert!(err.to_string().contains("Space set changed"));
    // Shape change refuses before any current comparison.
    let fewer = TopologySnapshot { displays: vec![] };
    let err = validate_goto_effect(&before, &fewer, 0, 12).unwrap_err();
    assert!(err.to_string().contains("display count"));
}

#[test]
fn removal_effect_requires_temp_gone_and_pinning_intact() {
    let pre = snapshot(vec![
        display(&[11, 12, 13], Some(11)),
        display(&[21], Some(21)),
    ]);
    let post = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    assert!(validate_removal_effect(&pre, &post, 13).is_ok());
    let err = validate_removal_effect(&pre, &pre, 13).unwrap_err();
    assert!(err.to_string().contains("still present"));
    let missing = snapshot(vec![display(&[11], Some(11)), display(&[21], Some(21))]);
    let err = validate_removal_effect(&pre, &missing, 13).unwrap_err();
    assert!(err.to_string().contains("beyond the temp removal"));
    let drifted = snapshot(vec![display(&[11, 12], Some(12)), display(&[21], Some(21))]);
    let err = validate_removal_effect(&pre, &drifted, 13).unwrap_err();
    assert!(err.to_string().contains("current changed"));
}

#[test]
fn exact_restoration_requires_ordered_baseline_equality() {
    let before = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    let same = snapshot(vec![display(&[11, 12], Some(11)), display(&[21], Some(21))]);
    assert!(require_exact_restoration(&before, &same).is_ok());
    let reordered = snapshot(vec![display(&[12, 11], Some(11)), display(&[21], Some(21))]);
    let err = require_exact_restoration(&before, &reordered).unwrap_err();
    assert!(err.to_string().contains("differ from baseline"));
    let switched = snapshot(vec![display(&[11, 12], Some(12)), display(&[21], Some(21))]);
    let err = require_exact_restoration(&before, &switched).unwrap_err();
    assert!(err.to_string().contains("current differs"));
    let renamed = TopologySnapshot {
        displays: vec![
            DisplaySpaces {
                identifier: Some("other-uuid".to_owned()),
                spaces: vec![11, 12],
                current: Some(11),
            },
            display(&[21], Some(21)),
        ],
    };
    let err = require_exact_restoration(&before, &renamed).unwrap_err();
    assert!(err.to_string().contains("identity changed"));
}

#[test]
fn removal_eligibility_guards_active_last_and_unknown() {
    let s = snapshot(vec![
        display(&[11, 12, 13], Some(11)),
        display(&[21], Some(21)),
    ]);
    assert_eq!(removal_eligibility(&s, 13), Ok(0));
    let active = snapshot(vec![display(&[11, 12, 13], Some(13))]);
    let err = removal_eligibility(&active, 13).unwrap_err();
    assert!(err.to_string().contains("current"));
    let lone_inactive = TopologySnapshot {
        displays: vec![DisplaySpaces {
            identifier: None,
            spaces: vec![13],
            current: Some(99),
        }],
    };
    let err = removal_eligibility(&lone_inactive, 13).unwrap_err();
    assert!(matches!(err, McError::Blocked(_)));
    assert!(err.to_string().contains("last Space"));
    let err = removal_eligibility(&s, 99).unwrap_err();
    assert!(matches!(err, McError::Ownership(_)));
    assert!(err.to_string().contains("ownership uncertain"));
}

#[test]
fn trust_and_session_gates_fail_closed() {
    assert!(require_ax_trust(Some(true)).is_ok());
    for t in [None, Some(false)] {
        let err = require_ax_trust(t).unwrap_err();
        assert!(matches!(err, McError::Blocked(_)));
        assert!(err.to_string().contains("not a SIP"));
    }
    let unlocked = crate::ffi::SessionState {
        login_done: Some(true),
        on_console: Some(true),
        screen_lock: crate::ffi::ScreenLockRead::Absent,
        lock: crate::ffi::SessionLock::Unlocked,
    };
    assert!(require_unlocked_session(&unlocked).is_ok());
    let locked = crate::ffi::SessionState {
        login_done: Some(true),
        on_console: Some(true),
        screen_lock: crate::ffi::ScreenLockRead::Locked,
        lock: crate::ffi::SessionLock::Locked,
    };
    assert!(require_unlocked_session(&locked).is_err());
    assert!(require_unlocked_session(&crate::ffi::SessionState::unknown()).is_err());
}

#[test]
fn dock_gate_matches_exactly_and_refuses_protected() {
    assert!(require_dock_bundle(DOCK_BUNDLE_ID).is_ok());
    assert!(require_dock_bundle("").is_err());
    assert!(require_dock_bundle("com.apple.finder").is_err());
    for b in [
        "com.mitchellh.ghostty",
        "com.apple.Terminal",
        "com.googlecode.iterm2",
    ] {
        let err = require_dock_bundle(b).unwrap_err();
        assert!(matches!(err, McError::Ownership(_)), "{b}");
    }
}

#[test]
fn budget_refuses_inside_reserve_past_deadline_and_past_cap() {
    // Zero-second budget: the reserve already covers the deadline, so even
    // the first main-phase spend refuses deterministically.
    let mut zero = ActionBudget::new(0);
    let err = zero.spend_main("probe").unwrap_err();
    assert!(matches!(err, McError::Blocked(_)));
    assert!(err.to_string().contains("cleanup reserve"));
    assert!(zero.spend_cleanup("probe").is_err());
    // Generous budget: exactly MC_MAX_ACTIONS dispatches, then the cap
    // binds (time is not the limiter here).
    let mut big = ActionBudget::new(3600);
    for _ in 0..MC_MAX_ACTIONS {
        big.spend_main("probe").expect("within cap and deadline");
    }
    let err = big.spend_main("one-more").unwrap_err();
    assert!(err.to_string().contains("past 32 dispatches"));
}

#[test]
fn shell_classifier_covers_dock_and_subnormal_layers_only() {
    use WindowClass::{Candidate, Shell};
    assert_eq!(classify_window("com.apple.dock", 0), Shell);
    assert_eq!(classify_window("com.apple.dock", 20), Shell);
    assert_eq!(classify_window("com.example.app", -1), Shell);
    // Ordinary apps — Ghostty included, Finder included — are candidates.
    for b in [
        "com.mitchellh.ghostty",
        "com.mitchellh.ghostty-beta",
        "com.apple.finder",
        "com.apple.Terminal",
        "com.example.app",
        "",
    ] {
        assert_eq!(classify_window(b, 0), Candidate, "{b}");
        assert_eq!(classify_window(b, 8), Candidate, "{b}");
    }
}

#[test]
fn ghostty_is_candidate_before_negative_layer_shell_rule() {
    use WindowClass::Candidate;
    // Regression: Ghostty windows (any layer, any case) are always
    // candidates, never shell chrome. A Ghostty window at a sub-normal
    // layer must not be skipped by the emptiness scan.
    for b in [
        "com.mitchellh.ghostty",
        "com.mitchellh.ghostty-beta",
        "COM.MITCHELLH.GHOSTTY",
    ] {
        for layer in [-100, -1, 0, 8] {
            assert_eq!(classify_window(b, layer), Candidate, "{b}@{layer}");
        }
    }
    // The emptiness gate must count a Ghostty window at a negative layer
    // as a candidate: member-of-temp blocks, unknown blocks, clear passes.
    let pairs = vec![(
        WindowMember {
            wid: 21,
            pid: 121,
            bundle: "com.mitchellh.ghostty".to_owned(),
            layer: -1,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Ok(vec![13]),
    )];
    let err = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    let pairs = vec![(
        WindowMember {
            wid: 22,
            pid: 122,
            bundle: "com.mitchellh.ghostty".to_owned(),
            layer: -1,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Err("withheld".to_owned()),
    )];
    assert!(verify_space_empty_members(&pairs, 13, &[11, 13]).is_err());
    let pairs = vec![(
        WindowMember {
            wid: 23,
            pid: 123,
            bundle: "com.mitchellh.ghostty".to_owned(),
            layer: -1,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Ok(vec![11]),
    )];
    let summary = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap();
    assert_eq!((summary.checked, summary.shell_skipped), (1, 0));
}

fn member(wid: u32, bundle: &str, layer: i32) -> (WindowMember, Result<Vec<u64>, String>) {
    (
        WindowMember {
            wid,
            pid: 100 + wid,
            bundle: bundle.to_owned(),
            layer,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Ok(vec![11]),
    )
}

#[test]
fn emptiness_gate_skips_shell_but_never_ghostty() {
    // Shell chrome (Dock, sub-normal layers) skips with a count.
    let pairs = vec![
        member(1, "com.apple.dock", 0),
        member(2, "com.apple.wallpaper.agent", -1),
        member(3, "com.example.app", 0),
    ];
    let summary = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap();
    assert_eq!(summary.checked, 1);
    assert_eq!(summary.shell_skipped, 2);
    // A candidate member of the temp blocks as non-empty.
    let pairs = vec![(
        WindowMember {
            wid: 7,
            pid: 107,
            bundle: "com.example.app".to_owned(),
            layer: 0,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Ok(vec![11, 13]),
    )];
    let err = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Unknown membership on a candidate blocks (never assumed empty).
    let pairs = vec![(
        WindowMember {
            wid: 8,
            pid: 108,
            bundle: "com.example.app".to_owned(),
            layer: 0,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Err("SLSCopySpacesForWindows returned null".to_owned()),
    )];
    let err = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap_err();
    assert!(err.to_string().contains("emptiness unknown"));
    // Ghostty windows are candidates: member blocks, unknown blocks.
    let pairs = vec![(
        WindowMember {
            wid: 9,
            pid: 109,
            bundle: "com.mitchellh.ghostty".to_owned(),
            layer: 0,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Ok(vec![13]),
    )];
    let err = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    let pairs = vec![(
        WindowMember {
            wid: 10,
            pid: 110,
            bundle: "com.mitchellh.ghostty".to_owned(),
            layer: 0,
            onscreen: true,
            w: 100.0,
            h: 100.0,
            exe: None,
            exe_alias: None,
        },
        Err("withheld".to_owned()),
    )];
    assert!(verify_space_empty_members(&pairs, 13, &[11, 13]).is_err());
    // Empty enumeration is vacuously clear with zero counts.
    let summary = verify_space_empty_members(&[], 13, &[11, 13]).unwrap();
    assert_eq!((summary.checked, summary.shell_skipped), (0, 0));
}

/// Full-evidence member builder for the narrow exemption tests.
#[allow(clippy::too_many_arguments)]
fn chrome(
    wid: u32,
    pid: u32,
    bundle: &str,
    layer: i32,
    onscreen: bool,
    w: f64,
    h: f64,
    exe: Option<&str>,
    spaces: Result<Vec<u64>, String>,
) -> (WindowMember, Result<Vec<u64>, String>) {
    (
        WindowMember {
            wid,
            pid,
            bundle: bundle.to_owned(),
            layer,
            onscreen,
            w,
            h,
            exe: exe.map(str::to_owned),
            exe_alias: None,
        },
        spaces,
    )
}

#[test]
fn compositor_exemption_needs_the_exact_executable() {
    // Representative shell shape: WindowServer-owned menubar backing, empty bundle,
    // kept layer, temp-only membership. The exact fresh executable path
    // exempts it with its identity evidence in the log.
    let pairs = vec![chrome(
        7,
        42,
        "",
        24,
        false,
        800.0,
        600.0,
        Some(WINDOWSERVER_EXE),
        Ok(vec![20]),
    )];
    let summary = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap();
    assert_eq!((summary.checked, summary.shell_skipped), (0, 1));
    assert_eq!(summary.excluded.len(), 1);
    let only = &summary.excluded[0];
    assert_eq!((only.wid, only.pid), (7, 42));
    assert_eq!(only.exe.as_deref(), Some(WINDOWSERVER_EXE));
    assert_eq!(only.reason, ShellReason::WindowServerCompositor);
    assert!(only.detail.contains("Resources/WindowServer"));
    // Same window with an unreadable executable: bundle absence, PID, and
    // layer never exempt on their own — it blocks as a candidate member.
    let pairs = vec![chrome(
        7,
        42,
        "",
        24,
        false,
        800.0,
        600.0,
        None,
        Ok(vec![20]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Same window with a merely similar executable: exact match or nothing.
    let pairs = vec![chrome(
        7,
        42,
        "",
        24,
        false,
        800.0,
        600.0,
        Some("/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServerX"),
        Ok(vec![20]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // An ordinary app at the same layer with the same membership blocks:
    // layers never skip generically.
    let pairs = vec![chrome(
        8,
        43,
        "com.example.app",
        24,
        false,
        800.0,
        600.0,
        None,
        Ok(vec![20]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
}

#[test]
fn compositor_alias_keeps_raw_observation_in_report() {
    // Normalized match (enumerator's job) still classifies Shell, and the
    // raw versioned alias rides in the detail — the report never claims
    // `None` meant a different owner.
    let alias =
        "/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer";
    let pairs = vec![(
        WindowMember {
            wid: 7,
            pid: 42,
            bundle: "".to_owned(),
            layer: 24,
            onscreen: false,
            w: 800.0,
            h: 600.0,
            exe: Some(WINDOWSERVER_EXE.to_owned()),
            exe_alias: Some(alias.to_owned()),
        },
        Ok(vec![20]),
    )];
    let summary = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap();
    assert_eq!((summary.checked, summary.shell_skipped), (0, 1));
    let only = &summary.excluded[0];
    assert_eq!(only.reason, ShellReason::WindowServerCompositor);
    assert!(only.detail.contains("canonicalized"));
    assert!(only.detail.contains("Versions/A"));
    // Same shape without the alias note keeps the plain detail.
    let pairs = vec![chrome(
        7,
        42,
        "",
        24,
        false,
        800.0,
        600.0,
        Some(WINDOWSERVER_EXE),
        Ok(vec![20]),
    )];
    let summary = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap();
    assert_eq!(
        summary.excluded[0].detail,
        format!("exe == {WINDOWSERVER_EXE}")
    );
}

#[test]
fn normalize_passes_through_exact_none_and_unrelated() {
    // Exact constant and unreadable reads never touch normalization.
    assert_eq!(
        normalize_windowserver_alias(Some(WINDOWSERVER_EXE.to_owned())),
        (Some(WINDOWSERVER_EXE.to_owned()), None)
    );
    assert_eq!(normalize_windowserver_alias(None), (None, None));
    // An unrelated executable passes through byte-identical on every
    // platform (no filesystem probe for non-WindowServer names).
    assert_eq!(
        normalize_windowserver_alias(Some("/usr/bin/other".to_owned())),
        (Some("/usr/bin/other".to_owned()), None)
    );
    // A WindowServer-named path that resolves nowhere stays raw and
    // unflagged: fail closed, never an exemption.
    assert_eq!(
        normalize_windowserver_alias(Some("/tmp/WindowServer-gone".to_owned())),
        (Some("/tmp/WindowServer-gone".to_owned()), None)
    );
}

#[cfg(unix)]
#[test]
fn versioned_alias_canonicalizes_without_guesses() {
    // Symlink farm mimicking the SkyLight layout: the versioned read and
    // the symlink form resolve identically; a nearby different file and a
    // missing path refuse.
    let dir = std::env::temp_dir().join(format!("omnitiler-ws-alias-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let versioned = dir.join("Versions").join("A").join("Resources");
    std::fs::create_dir_all(&versioned).unwrap();
    std::fs::write(versioned.join("WindowServer"), b"compositor").unwrap();
    std::fs::write(versioned.join("WindowServerX"), b"other").unwrap();
    std::os::unix::fs::symlink(&versioned, dir.join("Resources")).unwrap();
    let const_form = dir.join("Resources").join("WindowServer");
    let canon_const = std::fs::canonicalize(&const_form).unwrap();
    let raw_alias = versioned.join("WindowServer");
    assert!(canonical_exe_equals(
        raw_alias.to_str().unwrap(),
        &canon_const
    ));
    assert!(!canonical_exe_equals(
        versioned.join("WindowServerX").to_str().unwrap(),
        &canon_const
    ));
    assert!(!canonical_exe_equals(
        versioned.join("WindowServerGone").to_str().unwrap(),
        &canon_const
    ));
    assert!(!canonical_exe_equals("/usr/bin/other", &canon_const));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "macos")]
#[test]
fn live_versioned_alias_normalizes_to_const() {
    // Pins the representative shell shape: a versioned proc_pidpath read
    // for WindowServer normalizes to exactly WINDOWSERVER_EXE with
    // the raw alias retained.
    let alias =
        "/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer";
    assert_eq!(
        normalize_windowserver_alias(Some(alias.to_owned())),
        (Some(WINDOWSERVER_EXE.to_owned()), Some(alias.to_owned()))
    );
}

#[test]
fn sticky_chrome_exemption_needs_all_four_marks() {
    // Representative shell shape: AccessibilityUIServer, offscreen, zero area, sticky over
    // every user Space. All four marks together exempt it.
    let sticky = || {
        vec![chrome(
            7,
            42,
            A11Y_SERVER_BUNDLE,
            3,
            false,
            0.0,
            0.0,
            Some(
                "/System/Library/CoreServices/AccessibilityUIServer.app/Contents/MacOS/AccessibilityUIServer",
            ),
            Ok(vec![20, 10]),
        )]
    };
    let summary = verify_space_empty_members(&sticky(), 20, &[10, 20]).unwrap();
    assert_eq!((summary.checked, summary.shell_skipped), (0, 1));
    assert_eq!(summary.excluded.len(), 1);
    let only = &summary.excluded[0];
    assert_eq!((only.wid, only.pid), (7, 42));
    assert_eq!(only.bundle, A11Y_SERVER_BUNDLE);
    assert_eq!(only.reason, ShellReason::StickyAccessibilityChrome);
    assert!(only.detail.contains("sticky over 2 user Spaces"));
    // Onscreen with the same owner/area/membership: still a candidate.
    let mut lit = sticky();
    lit[0].0.onscreen = true;
    let err = verify_space_empty_members(&lit, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Nonzero area with the same owner/visibility/membership: candidate.
    let mut lit = sticky();
    lit[0].0.w = 10.0;
    lit[0].0.h = 10.0;
    let err = verify_space_empty_members(&lit, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Non-sticky (missing one user Space): candidate, even offscreen/zero.
    let pairs = vec![chrome(
        7,
        42,
        A11Y_SERVER_BUNDLE,
        3,
        false,
        0.0,
        0.0,
        None,
        Ok(vec![20]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Unknown membership: never assumed sticky, blocks as unknown.
    let pairs = vec![chrome(
        7,
        42,
        A11Y_SERVER_BUNDLE,
        3,
        false,
        0.0,
        0.0,
        None,
        Err("withheld".to_owned()),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("emptiness unknown"));
    // Changed owner with otherwise identical marks: candidate.
    let pairs = vec![chrome(
        7,
        42,
        "com.example.a11y",
        3,
        false,
        0.0,
        0.0,
        None,
        Ok(vec![20, 10]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Empty user set: stickiness unprovable, candidate.
    let err = verify_space_empty_members(&sticky(), 20, &[]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
}

#[test]
fn ordinary_sticky_windows_and_ghostty_always_block() {
    // An ordinary sticky app window (kept layer, member of both user
    // Spaces) is not chrome: it blocks.
    let pairs = vec![chrome(
        300,
        700,
        "com.apple.finder",
        0,
        true,
        800.0,
        600.0,
        None,
        Ok(vec![20, 10]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Ghostty stays a candidate even with a compositor executable path and
    // a sub-normal layer: ownership is checked before every exemption.
    let pairs = vec![chrome(
        301,
        701,
        "com.mitchellh.ghostty",
        -1,
        false,
        0.0,
        0.0,
        Some(WINDOWSERVER_EXE),
        Ok(vec![20, 10]),
    )];
    let err = verify_space_empty_members(&pairs, 20, &[10, 20]).unwrap_err();
    assert!(err.to_string().contains("not empty"));
    // Ghostty with unknown membership still blocks as unknown, never shell.
    let pairs = vec![chrome(
        302,
        702,
        "com.mitchellh.ghostty",
        0,
        true,
        100.0,
        100.0,
        None,
        Err("withheld".to_owned()),
    )];
    assert!(verify_space_empty_members(&pairs, 20, &[10, 20]).is_err());
}

#[test]
fn base_rules_still_exempt_with_named_reasons() {
    let pairs = vec![
        member(1, "com.apple.dock", 0),
        member(2, "com.apple.wallpaper.agent", -1),
        member(3, "com.example.app", 0),
    ];
    let summary = verify_space_empty_members(&pairs, 13, &[11, 13]).unwrap();
    assert_eq!((summary.checked, summary.shell_skipped), (1, 2));
    assert_eq!(summary.excluded.len(), 2);
    assert_eq!(summary.excluded[0].reason, ShellReason::DockExact);
    assert_eq!(summary.excluded[1].reason, ShellReason::SubNormalLayer);
    assert!(summary.excluded[0].detail.contains("com.apple.dock"));
}

fn focus_legs(ws: i32, wid: Option<u32>) -> FocusLegs {
    FocusLegs {
        ws_pid: Some(ws),
        ws_bundle: Some("com.example.app".to_owned()),
        slps_pid: Some(ws),
        direct: Some(crate::ffi::DirectFocusRead {
            pid: ws,
            ax_pid: Some(ws),
            focused_status: "kAXErrorSuccess".to_owned(),
            focused_wid: wid,
            main_status: "kAXErrorSuccess".to_owned(),
            main_wid: wid,
            selected_wid: wid,
            selected_source: "focused".to_owned(),
        }),
        direct_readable: true,
    }
}

#[test]
fn focus_restoration_needs_full_readable_agreement() {
    let base = focus_legs(4242, Some(7));
    assert!(focus_restored(&base, &base));
    // Workspace drift refuses even with the bundle unchanged.
    let mut drift = base.clone();
    drift.ws_pid = Some(4343);
    assert!(!focus_restored(&base, &drift));
    // Bundle-only match without the SLPS leg is unrestored.
    let mut no_slps = base.clone();
    no_slps.slps_pid = None;
    assert!(!focus_restored(&base, &no_slps));
    // Unreadable direct leg on either side is unrestored.
    let mut unreadable = base.clone();
    unreadable.direct_readable = false;
    assert!(!focus_restored(&base, &unreadable));
    assert!(!focus_restored(&unreadable, &base));
    // Direct window disagreement refuses.
    let other = focus_legs(4242, Some(9));
    assert!(!focus_restored(&base, &other));
    // Genuinely windowless-but-readable frontmost restores (None == None
    // with the readable flag set on both sides).
    let bare = focus_legs(4242, None);
    assert!(focus_restored(&bare, &bare));
}

#[test]
fn recover_topology_accepts_exact_ordered_pair_only() {
    let s = snapshot(vec![display(&[11, 20], Some(20))]);
    assert!(validate_recover_topology(&s, 0, 11, 20).is_ok());
    // Already restored (current back on the original) still adopts.
    let s = snapshot(vec![display(&[11, 20], Some(11))]);
    assert!(validate_recover_topology(&s, 0, 11, 20).is_ok());
    // Swapped order refuses: the adopted shape is [original, temp].
    let s = snapshot(vec![display(&[20, 11], Some(20))]);
    let err = validate_recover_topology(&s, 0, 11, 20).unwrap_err();
    assert!(err.to_string().contains("ordered [original=11, temp=20]"));
    // Extra Space refuses (not exactly one adopted temp).
    let s = snapshot(vec![display(&[11, 20, 67], Some(20))]);
    assert!(validate_recover_topology(&s, 0, 11, 20).is_err());
    // Missing temp refuses.
    let s = snapshot(vec![display(&[11], Some(11))]);
    assert!(validate_recover_topology(&s, 0, 11, 20).is_err());
    // Current on an unrelated Space refuses as drift.
    let s = snapshot(vec![display(&[11, 20], Some(99))]);
    let err = validate_recover_topology(&s, 0, 11, 20).unwrap_err();
    assert!(err.to_string().contains("drifted"));
    // Temp identical to original refuses (nothing to distinguish).
    let s = snapshot(vec![display(&[11], Some(11))]);
    let err = validate_recover_topology(&s, 0, 11, 11).unwrap_err();
    assert!(err.to_string().contains("equals temp"));
    // Unknown display refuses.
    let s = snapshot(vec![display(&[11, 20], Some(20))]);
    assert!(validate_recover_topology(&s, 3, 11, 20).is_err());
    // Ambiguous cross-display duplication refuses as uncertain ownership.
    let dup = TopologySnapshot {
        displays: vec![
            display(&[11, 20], Some(20)),
            DisplaySpaces {
                identifier: Some("uuid-other".to_owned()),
                spaces: vec![20],
                current: Some(20),
            },
        ],
    };
    let err = validate_recover_topology(&dup, 0, 11, 20).unwrap_err();
    assert!(matches!(err, McError::Ownership(_)));
}

fn ghostty_legs(ws: i32, wid: Option<u32>) -> FocusLegs {
    FocusLegs {
        ws_pid: Some(ws),
        ws_bundle: Some("com.mitchellh.ghostty".to_owned()),
        slps_pid: Some(ws),
        direct: Some(crate::ffi::DirectFocusRead {
            pid: ws,
            ax_pid: Some(ws),
            focused_status: "kAXErrorSuccess".to_owned(),
            focused_wid: wid,
            main_status: "kAXErrorSuccess".to_owned(),
            main_wid: wid,
            selected_wid: wid,
            selected_source: "focused".to_owned(),
        }),
        direct_readable: true,
    }
}

#[test]
fn ghostty_return_needs_agreement_on_ghostty_with_window() {
    assert!(focus_ghostty_returned(&ghostty_legs(4242, Some(7))));
    // Workspace/SLPS drift refuses.
    let mut drift = ghostty_legs(4242, Some(7));
    drift.slps_pid = Some(4343);
    assert!(!focus_ghostty_returned(&drift));
    // Non-Ghostty frontmost refuses even with full agreement.
    let mut finder = ghostty_legs(4242, Some(7));
    finder.ws_bundle = Some("com.apple.finder".to_owned());
    assert!(!focus_ghostty_returned(&finder));
    // Unreadable direct leg refuses.
    let mut unreadable = ghostty_legs(4242, Some(7));
    unreadable.direct_readable = false;
    assert!(!focus_ghostty_returned(&unreadable));
    // Missing selected window refuses (windowless is not a return).
    assert!(!focus_ghostty_returned(&ghostty_legs(4242, None)));
    // Zero selected window refuses.
    assert!(!focus_ghostty_returned(&ghostty_legs(4242, Some(0))));
    // AX identity joining another PID refuses.
    let mut foreign = ghostty_legs(4242, Some(7));
    if let Some(d) = foreign.direct.as_mut() {
        d.ax_pid = Some(4343);
    }
    assert!(!focus_ghostty_returned(&foreign));
}

#[test]
fn ghostty_frames_compare_exactly() {
    let g = GhosttyWindow {
        wid: 7,
        pid: 4242,
        layer: 0,
        onscreen: true,
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        spaces: Some(vec![11]),
    };
    assert_eq!(g, g.clone());
    let mut moved = g.clone();
    moved.x = 9.0;
    assert_ne!(g, moved);
    let json = serde_json::to_string(&g).unwrap();
    assert!(!json.contains("secret"), "no title field exists to leak");
}

#[test]
fn action_vocabulary_has_no_window_or_synthetic_variant() {
    // Closed vocabulary: every variant must be a Space/inspect/cleanup
    // step. A new window or synthetic-input variant fails this test by
    // exhaustiveness — the compiler names the new variant.
    for action in [
        McAction::Snapshot,
        McAction::Gate,
        McAction::AddSpace,
        McAction::GotoSpace,
        McAction::RemoveSpace,
        McAction::Cleanup,
    ] {
        let name = action.as_str();
        assert!(!name.contains("window"), "{name}");
        assert!(!name.contains("synthetic"), "{name}");
        assert!(!name.contains("post"), "{name}");
        assert!(!name.contains("focus-write"), "{name}");
    }
}

#[test]
fn sanitized_layouts_drop_identifiers() {
    let s = snapshot(vec![display(&[3, 1], Some(3))]);
    let layout = s.sanitized();
    assert_eq!(layout.space_ids, vec![1, 3]);
    assert_eq!(layout.space_count, 2);
    let ordered = s.ordered_layouts();
    assert_eq!(ordered.len(), 1);
    assert_eq!(ordered[0].index, 0);
    assert_eq!(ordered[0].spaces, vec![3, 1]);
    let json = serde_json::to_string(&ordered).unwrap();
    assert!(!json.contains("uuid-test"));
}

#[test]
fn error_report_preserves_temp_preimages_and_presence() {
    let report = McErrorReport {
        stage: "remove-temp".to_owned(),
        action: McAction::RemoveSpace.as_str().to_owned(),
        error: "blocked: temp Space 13 still present".to_owned(),
        temp_space: Some(13),
        layout_before: Some(McLayout {
            space_ids: vec![11, 12],
            current: vec![11],
            space_count: 2,
        }),
        layout_after: None,
        currents_restored: false,
        mc_open_after: Some(true),
        caffeinate_reaped: true,
        ghostty_before: None,
        ghostty_after: None,
        focus_before: None,
        focus_after: None,
        notes: vec!["MC closed".to_owned()],
        latencies: vec![LegLatency {
            leg: "remove".to_owned(),
            ms: 1234,
        }],
    };
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("13"));
    assert!(json.contains("remove-temp"));
    assert!(json.contains("MC closed"));
}

fn stable_window(wid: u32, onscreen: bool, spaces: Option<Vec<u64>>) -> GhosttyWindow {
    GhosttyWindow {
        wid,
        pid: 4242,
        layer: 0,
        onscreen,
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        spaces,
    }
}

#[test]
fn cross_space_stability_ignores_onscreen_but_pins_identity() {
    // The CURRENT-Space switch flips onscreen for windows confined to one
    // Space; membership-as-sets, bounds, and identity must still match.
    let before = vec![
        stable_window(7, false, Some(vec![11])),
        stable_window(9, false, Some(vec![11, 12])),
    ];
    let after = vec![
        stable_window(7, true, Some(vec![11])),
        stable_window(9, true, Some(vec![12, 11])),
    ];
    assert!(ghostty_stable_across_switch(&before, &after));
    // Exact lifecycle equality still distinguishes onscreen (the B0→B2
    // battery is intentionally stricter than this cross-Space check).
    assert_ne!(before, after);
    // Window-set change refuses.
    assert!(!ghostty_stable_across_switch(&before, &after[1..]));
    // PID drift refuses.
    let mut pid_drift = after.clone();
    pid_drift[0].pid = 4343;
    assert!(!ghostty_stable_across_switch(&before, &pid_drift));
    // Bounds drift refuses.
    let mut moved = after.clone();
    moved[1].x = 99.0;
    assert!(!ghostty_stable_across_switch(&before, &moved));
    // Membership drift refuses (order alone does not).
    let mut rehomed = after.clone();
    rehomed[0].spaces = Some(vec![12]);
    assert!(!ghostty_stable_across_switch(&before, &rehomed));
    // Layer drift refuses.
    let mut relayered = after.clone();
    relayered[1].layer = 8;
    assert!(!ghostty_stable_across_switch(&before, &relayered));
    // Empty both sides is vacuously stable.
    assert!(ghostty_stable_across_switch(&[], &[]));
}

#[test]
fn mismatch_detail_distinguishes_geometry_from_unknown_membership() {
    let before = vec![stable_window(7, false, Some(vec![11]))];
    let after = vec![stable_window(7, true, Some(vec![11]))];
    // Onscreen-only flip: stable, no detail.
    assert!(ghostty_mismatch_detail(&before, &after).is_none());
    // Unreadable membership on one side: unknown, named as such.
    let mut unknown = after.clone();
    unknown[0].spaces = None;
    let detail = ghostty_mismatch_detail(&before, &unknown).unwrap();
    assert!(detail.contains("unreadable"));
    // Real geometry drift: bounds named.
    let mut moved = after.clone();
    moved[0].x = 99.0;
    assert!(
        ghostty_mismatch_detail(&before, &moved)
            .unwrap()
            .contains("bounds")
    );
    // Count change named.
    assert!(
        ghostty_mismatch_detail(&before, &[])
            .unwrap()
            .contains("count")
    );
}
