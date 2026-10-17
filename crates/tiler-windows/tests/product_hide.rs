#![forbid(unsafe_code)]
use tiler_windows::model::{
    LEDGER_SCHEMA_VERSION, ProcessIdentity, ProductShowRestore, RecoveryLedger, WindowClaimKind,
    WindowIdentity, WindowShowState, generate_claim_tag, parse_ledger, product_show_restore,
    valid_claim_tag, validate_ledger, watcher_may_restore,
};
use tiler_windows::product_hide::{
    ProductTeardown, committed_has_same_claim, parse_watcher_ready, product_claim,
    project_topmost_hide_allowed, render_watcher_ready, shell_class_excluded, teardown_keep_ledger,
    watcher_ready_filename,
};

fn owner() -> ProcessIdentity {
    ProcessIdentity {
        pid: 100,
        process_creation: "0123456789abcdef".to_owned(),
        user_sid: "S-owner".to_owned(),
        session_id: 1,
        exe_path: "C:\\tools\\tiler-windows.exe".to_owned(),
    }
}

fn product_window() -> WindowIdentity {
    WindowIdentity {
        hwnd: 0xBEEF,
        process: ProcessIdentity {
            pid: 200,
            process_creation: "c-200".to_owned(),
            user_sid: "S-owner".to_owned(),
            session_id: 1,
            exe_path: "C:\\apps\\notepad.exe".to_owned(),
        },
        tag: "0123456789abcde1".to_owned(),
        kind: WindowClaimKind::Product,
        show: WindowShowState::default(),
    }
}

#[test]
fn nonce_format_and_generator() {
    assert!(valid_claim_tag("0123456789abcdef"));
    assert!(!valid_claim_tag(""));
    assert!(!valid_claim_tag("0000000000000000"));
    assert!(!valid_claim_tag("ABCDEF0123456789"));
    assert!(!valid_claim_tag("short"));
    for _ in 0..32 {
        let tag = generate_claim_tag(1234);
        assert!(valid_claim_tag(&tag), "generated {tag}");
    }
    let a = generate_claim_tag(42);
    let b = generate_claim_tag(42);
    assert_ne!(a, b, "per-hide nonces must differ");
}

#[test]
fn shell_exclusion_in_use() {
    assert!(shell_class_excluded("Shell_TrayWnd"));
    assert!(shell_class_excluded("Progman"));
    assert!(!shell_class_excluded("Notepad"));
}

#[test]
fn teardown_residue_rule() {
    use ProductTeardown::{AlreadyVisible, Retired, Revealed, Uncertain};
    assert!(!teardown_keep_ledger(&[]));
    assert!(!teardown_keep_ledger(&[Revealed, AlreadyVisible, Retired]));
    assert!(teardown_keep_ledger(&[Revealed, Uncertain]));
    assert!(teardown_keep_ledger(&[Uncertain]));
}

#[test]
fn watcher_binding_and_ready_format() {
    let captured = owner();
    assert!(watcher_may_restore(&captured, &captured));
    let mut replaced = captured.clone();
    replaced.process_creation = "ffffffffffffffff".to_owned();
    assert!(!watcher_may_restore(&captured, &replaced));
    let mut other_exe = captured.clone();
    other_exe.exe_path = "C:\\tools\\other.exe".to_owned();
    assert!(!watcher_may_restore(&captured, &other_exe));
    assert_eq!(watcher_ready_filename("abc"), "watcher-abc.ready");
    let rendered = render_watcher_ready("owner-hex", 4242, "watcher-hex");
    let parsed = parse_watcher_ready(&rendered).expect("roundtrip");
    assert_eq!(
        parsed,
        ("owner-hex".to_owned(), 4242, "watcher-hex".to_owned())
    );
    assert!(parse_watcher_ready("").is_none());
    assert!(parse_watcher_ready("only-owner\n").is_none());
    assert!(parse_watcher_ready("o\n0\nw\n").is_none());
    assert!(parse_watcher_ready("o\nnot-a-pid\nw\n").is_none());
    assert!(parse_watcher_ready("o\n12\n\n").is_none());
}

#[test]
fn product_claim_builder_sets_kind() {
    let claim = product_claim(0x1234, owner(), "0123456789abcde2".to_owned());
    assert_eq!(claim.kind, WindowClaimKind::Product);
    assert_eq!(claim.hwnd, 0x1234);
}

#[test]
fn orphan_cleanup_retains_only_same_hwnd_tag_claim() {
    let claim = product_window();
    // Same HWND+tag product claim retains: never cleanup.
    assert!(committed_has_same_claim(
        std::slice::from_ref(&claim),
        claim.hwnd,
        &claim.tag
    ));
    // Different tag does not retain.
    assert!(!committed_has_same_claim(
        std::slice::from_ref(&claim),
        claim.hwnd,
        "0123456789abcde9"
    ));
    // Different HWND does not retain.
    assert!(!committed_has_same_claim(
        std::slice::from_ref(&claim),
        0x1234,
        &claim.tag
    ));
    // Same HWND+tag but helper kind does not retain a product orphan.
    let mut helper = claim.clone();
    helper.kind = WindowClaimKind::Helper;
    assert!(!committed_has_same_claim(
        std::slice::from_ref(&helper),
        helper.hwnd,
        &helper.tag
    ));
    // Empty ledger never retains.
    assert!(!committed_has_same_claim(&[], claim.hwnd, &claim.tag));
}

#[test]
fn v4_roundtrip_and_older_refusal() {
    let ledger = RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: vec![product_window()],
        mouse_snap: None,
    };
    assert_eq!(LEDGER_SCHEMA_VERSION, 4);
    validate_ledger(&ledger).expect("v4 product ledger validates");
    let json = serde_json::to_string(&ledger).expect("json");
    assert_eq!(parse_ledger(&json).expect("parse"), ledger);
    // Empty v4 product tag cannot bind HWND reuse.
    let mut bad = ledger.clone();
    bad.windows[0].tag.clear();
    assert!(validate_ledger(&bad).is_err());
    // v2 carrying a product claim is version confusion.
    let mut confused = ledger.clone();
    confused.v = 2;
    assert!(validate_ledger(&confused).is_err());
    // v2 helper-only fixtures without `kind` still parse as helper.
    let legacy = serde_json::json!({
        "v": 2,
        "owner": owner(),
        "windows": [{"hwnd": 1, "process": {
            "pid": 200, "process_creation": "c-200", "user_sid": "S-owner",
            "session_id": 1, "exe_path": "C:\\apps\\a.exe"},
            "tag": "0123456789abcde3"}],
        "mouse_snap": null,
    });
    let parsed = parse_ledger(&legacy.to_string()).expect("legacy v2 parses");
    assert_eq!(parsed.windows[0].kind, WindowClaimKind::Helper);
}

#[test]
fn v1_v2_v3_parse_with_normal_restore_default() {
    // Ledgers written before show-state capture carry no `show` field and
    // restore as normal; the reader accepts every prior version unchanged.
    // v1/v2 never carried product claims, so their fixtures stay helper-only.
    let helper_window = serde_json::json!({
        "hwnd": 1,
        "process": {
            "pid": 200, "process_creation": "c-200",
            "user_sid": "S-owner", "session_id": 1,
            "exe_path": "C:\\apps\\a.exe"},
        "tag": "tag-1",
        "kind": "helper",
    });
    let product_window_json = serde_json::json!({
        "hwnd": 1,
        "process": {
            "pid": 200, "process_creation": "c-200",
            "user_sid": "S-owner", "session_id": 1,
            "exe_path": "C:\\apps\\a.exe"},
        "tag": "0123456789abcde3",
        "kind": "product",
    });
    for (v, window) in [
        (1, helper_window.clone()),
        (2, helper_window.clone()),
        (3, product_window_json),
    ] {
        let fixture = serde_json::json!({
            "v": v,
            "owner": owner(),
            "windows": [window],
            "mouse_snap": null,
        });
        let parsed = parse_ledger(&fixture.to_string()).expect("prior version parses");
        assert_eq!(parsed.windows[0].show, WindowShowState::default());
        assert_eq!(
            product_show_restore(parsed.windows[0].show),
            ProductShowRestore::ShowNormal
        );
    }
}

#[test]
fn v4_show_preimage_roundtrip() {
    let mut minimized = product_window();
    minimized.show = WindowShowState {
        minimized: true,
        maximized: false,
    };
    let mut maximized = product_window();
    maximized.hwnd = 0xBEEF + 1;
    maximized.tag = "0123456789abcde2".to_owned();
    maximized.show = WindowShowState {
        minimized: false,
        maximized: true,
    };
    let ledger = RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: vec![minimized, maximized],
        mouse_snap: None,
    };
    validate_ledger(&ledger).expect("v4 show preimages validate");
    let json = serde_json::to_string(&ledger).expect("json");
    let parsed = parse_ledger(&json).expect("v4 show preimages parse");
    assert_eq!(parsed, ledger);
    assert_eq!(
        product_show_restore(parsed.windows[0].show),
        ProductShowRestore::ShowMinimized
    );
    assert_eq!(
        product_show_restore(parsed.windows[1].show),
        ProductShowRestore::ShowMaximized
    );
}

#[test]
fn v3_reader_refuses_v4_show_ledger() {
    // The v3-era version gate (1/2/3 only) refuses a v4 ledger outright, so
    // an older reader can never silently ignore the show preimage, restore
    // retained windows unminimized, and delete the ledger.
    fn validate_as_v3_reader(ledger: &RecoveryLedger) -> bool {
        matches!(ledger.v, 1..=3)
    }
    let ledger = RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: vec![product_window()],
        mouse_snap: None,
    };
    assert_eq!(LEDGER_SCHEMA_VERSION, 4);
    assert!(!validate_as_v3_reader(&ledger));
    let v3 = RecoveryLedger { v: 3, ..ledger };
    assert!(validate_as_v3_reader(&v3));
}

#[test]
fn show_restore_intent_choices() {
    assert_eq!(
        product_show_restore(WindowShowState {
            minimized: false,
            maximized: false,
        }),
        ProductShowRestore::ShowNormal
    );
    assert_eq!(
        product_show_restore(WindowShowState {
            minimized: true,
            maximized: false,
        }),
        ProductShowRestore::ShowMinimized
    );
    assert_eq!(
        product_show_restore(WindowShowState {
            minimized: false,
            maximized: true,
        }),
        ProductShowRestore::ShowMaximized
    );
    // Corrupt-or-raced both-true preimage stays minimized, never unminimizes.
    assert_eq!(
        product_show_restore(WindowShowState {
            minimized: true,
            maximized: true,
        }),
        ProductShowRestore::ShowMinimized
    );
}

#[test]
fn v3_product_tag_format_enforced() {
    // Non-hex, uppercase, zero, short, and empty product tags never commit.
    for bad_tag in [
        "tag-1",
        "ABCDEF0123456789",
        "0000000000000000",
        "",
        "short",
        "0123456789abcdeg",
    ] {
        let mut window = product_window();
        window.tag = bad_tag.to_owned();
        let ledger = RecoveryLedger {
            v: LEDGER_SCHEMA_VERSION,
            owner: owner(),
            windows: vec![window],
            mouse_snap: None,
        };
        assert!(
            validate_ledger(&ledger).is_err(),
            "product tag must be rejected: {bad_tag:?}"
        );
    }
    // Helper claims keep the nonempty rule on v4.
    let mut helper = product_window();
    helper.kind = WindowClaimKind::Helper;
    helper.tag = "tag-1".to_owned();
    let ledger = RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: vec![helper],
        mouse_snap: None,
    };
    validate_ledger(&ledger).expect("helper nonempty tag still valid on v3");
}

#[test]
fn project_topmost_gate_only_authenticates_raised_band() {
    // Normal tile with no band: no bypass needed or granted.
    assert!(!project_topmost_hide_allowed(None, false));
    assert!(!project_topmost_hide_allowed(Some(false), false));
    assert!(!project_topmost_hide_allowed(Some(true), false));
    // Project-raised band from a non-topmost prior: the only allowed case.
    assert!(project_topmost_hide_allowed(Some(false), true));
    // Preexisting topmost and unknown history refuse even when live.
    assert!(!project_topmost_hide_allowed(Some(true), true));
    assert!(!project_topmost_hide_allowed(None, true));
}

#[cfg(windows)]
mod native_topmost_hide {
    use tiler_windows::ffi::FfiOwnedTestWindow;
    use tiler_windows::model::ProcessIdentity;
    use tiler_windows::product_hide::{project_topmost_hide_allowed, sys};

    struct TestWindow {
        owned: FfiOwnedTestWindow,
    }

    impl TestWindow {
        fn create(topmost: bool) -> TestWindow {
            let owned = if topmost {
                FfiOwnedTestWindow::create_topmost("hide-topmost-test", 0, 0, 100, 100)
            } else {
                FfiOwnedTestWindow::create("hide-topmost-test", 0, 0, 100, 100, false)
            }
            .expect("invisible own-process test window creates");
            assert!(
                owned.hwnd() != 0,
                "invisible own-process test window creates"
            );
            TestWindow { owned }
        }

        fn hwnd(&self) -> u64 {
            self.owned.hwnd()
        }

        fn refusal_code(status: sys::CandidateStatus) -> Option<&'static str> {
            match status {
                sys::CandidateStatus::Admissible(_)
                | sys::CandidateStatus::Retained(_)
                | sys::CandidateStatus::Absent
                | sys::CandidateStatus::Uncertain => None,
                sys::CandidateStatus::Refused(code) => Some(code),
            }
        }
    }

    fn fake_owner() -> ProcessIdentity {
        let real = tiler_windows::native::current_identity().expect("current identity reads");
        let pid = real.pid.wrapping_add(0x1_0000).max(1);
        assert_ne!(pid, real.pid, "fake owner differs from window owner");
        ProcessIdentity {
            pid,
            process_creation: real.process_creation.clone(),
            user_sid: real.user_sid.clone(),
            session_id: real.session_id,
            exe_path: real.exe_path.clone(),
        }
    }

    fn runner_is_medium() -> bool {
        // Own-process windows inherit the runner integrity level, and the
        // integrity gate (product_hide.rs) precedes the topmost/visibility
        // gates. Medium runners reach those gates; elevated runners stop at
        // "integrity" before them.
        tiler_windows::native::current_integrity_level()
            .ok()
            .is_some_and(tiler_windows::lifecycle::is_medium_rid)
    }

    #[test]
    fn normal_tile_without_band_never_hits_topmost_gate() {
        let window = TestWindow::create(false);
        let owner = fake_owner();
        // Actual classification: an invisible non-topmost window reaches the
        // visibility gate, proving no topmost refusal without a band.
        // Non-medium runners stop at the earlier integrity gate instead.
        let expected = if runner_is_medium() {
            Some("not-visible")
        } else {
            Some("integrity")
        };
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate(window.hwnd(), &owner)),
            expected
        );
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate_for_hide(
                window.hwnd(),
                &owner,
                false
            )),
            expected
        );
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate_for_hide(
                window.hwnd(),
                &owner,
                true
            )),
            expected
        );
        assert!(!project_topmost_hide_allowed(None, false));
    }

    #[test]
    fn preexisting_band_refuses_without_bypass() {
        let window = TestWindow::create(true);
        let owner = fake_owner();
        // Blanket path and hide path without the flag both refuse topmost.
        // Non-medium runners stop at the earlier integrity gate instead.
        let expected = if runner_is_medium() {
            Some("topmost")
        } else {
            Some("integrity")
        };
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate(window.hwnd(), &owner)),
            expected
        );
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate_for_hide(
                window.hwnd(),
                &owner,
                false
            )),
            expected
        );
        // Unknown history and preexisting prior never authenticate.
        assert!(!project_topmost_hide_allowed(None, true));
        assert!(!project_topmost_hide_allowed(Some(true), true));
    }

    #[test]
    fn project_raised_band_bypasses_only_with_flag() {
        let window = TestWindow::create(true);
        let owner = fake_owner();
        // Same live topmost window: the flag moves actual classification past
        // the topmost gate to the next gate (invisible, so not-visible),
        // proving the bypass without clearing the band. Non-medium runners
        // stop at the earlier integrity gate even with the flag.
        let expected = if runner_is_medium() {
            Some("not-visible")
        } else {
            Some("integrity")
        };
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate_for_hide(
                window.hwnd(),
                &owner,
                true
            )),
            expected
        );
        assert!(project_topmost_hide_allowed(Some(false), true));
    }

    #[test]
    fn marker_alone_never_authorizes_recycled_identity_refuses() {
        let window = TestWindow::create(true);
        let owner = fake_owner();
        sys::install_float_intent_marker(window.hwnd()).expect("float marker installs");
        // A present float marker without the runtime flag still refuses
        // topmost: markers never authorize hiding. A recycled identity has no
        // exact runtime key (`None`), so it refuses the same way. Non-medium
        // runners stop at the earlier integrity gate instead.
        let refused = if runner_is_medium() {
            Some("topmost")
        } else {
            Some("integrity")
        };
        let bypassed = if runner_is_medium() {
            Some("not-visible")
        } else {
            Some("integrity")
        };
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate(window.hwnd(), &owner)),
            refused
        );
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate_for_hide(
                window.hwnd(),
                &owner,
                false
            )),
            refused
        );
        assert!(!project_topmost_hide_allowed(None, true));
        // The authenticated flag still bypasses with the marker present,
        // proving the flag (not the marker) is the authority.
        assert_eq!(
            TestWindow::refusal_code(sys::classify_candidate_for_hide(
                window.hwnd(),
                &owner,
                true
            )),
            bypassed
        );
        let _ = sys::remove_float_intent_marker(window.hwnd());
    }
}
