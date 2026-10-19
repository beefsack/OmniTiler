#![forbid(unsafe_code)]
use super::reconcile::writable_tokens_limited;
use super::{
    TickFlow, clear_pending_seams, geometry_write_allowed, inject_pending_elevated,
    inject_pending_foreground, inject_pending_observe, inject_pending_suspend, inject_pending_tag,
    is_limited_fullscreen, pending_apply_writables, pending_counters, pending_native_transfers,
    reconcile_limited_tick, run_active_tick,
};
use crate::workspace_owner::{ForegroundFacts, ForegroundVeto, ForegroundVetoReason};

fn veto_read(block: bool, reason: ForegroundVetoReason) -> ForegroundVeto {
    ForegroundVeto { block, reason }
}

fn facts_covering() -> ForegroundFacts {
    ForegroundFacts {
        valid: true,
        is_desktop: false,
        visible: true,
        captioned: false,
        dwm_readable: true,
        cloak_readable: true,
        cloaked: false,
        covers_monitor: true,
    }
}

#[test]
fn limited_policy_excludes_only_foreground() {
    use crate::workspace_owner::classify_foreground;
    let covering = facts_covering();
    let veto = classify_foreground(covering);
    assert!(veto.block && veto.reason == ForegroundVetoReason::Fullscreen);
    let read = super::settings::ForegroundRead {
        veto,
        desktop: false,
        visible: true,
        captioned: false,
        dwm_readable: true,
        cloaked: false,
    };
    assert!(is_limited_fullscreen(&read));
    // Foreground itself never writes; another window may.
    assert!(!geometry_write_allowed(&read, 0x9911, 0x9911));
    assert!(geometry_write_allowed(&read, 0x1001, 0x9911));
    // Unknown foreground fails closed: nothing writes.
    assert!(!geometry_write_allowed(&read, 0x1001, 0));
    // Other blocking shapes stay fully suspended.
    for reason in [
        ForegroundVetoReason::Invalid,
        ForegroundVetoReason::Unreadable,
    ] {
        let blocked = veto_read(true, reason);
        let other = super::settings::ForegroundRead {
            veto: blocked,
            desktop: false,
            visible: true,
            captioned: false,
            dwm_readable: true,
            cloaked: false,
        };
        assert!(
            !is_limited_fullscreen(&other),
            "{reason:?} must not be limited"
        );
        assert!(!geometry_write_allowed(&other, 0x1001, 0x9911));
        assert!(!geometry_write_allowed(&other, 0x9911, 0x9911));
    }
    // Non-blocking shapes always allow through revalidation.
    for reason in [
        ForegroundVetoReason::None,
        ForegroundVetoReason::ManagedOverlay,
        ForegroundVetoReason::Desktop,
    ] {
        let open = veto_read(false, reason);
        let other = super::settings::ForegroundRead {
            veto: open,
            desktop: false,
            visible: true,
            captioned: false,
            dwm_readable: true,
            cloaked: false,
        };
        assert!(!is_limited_fullscreen(&other));
        assert!(geometry_write_allowed(&other, 0x1001, 0x9911));
    }
}

fn limited_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 5009,
        process_creation: "creation-limited-me".to_owned(),
        user_sid: "S-1-limited".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    }
}

fn limited_frame(x: i32, y: i32, w: i32, h: i32) -> tiler_core::geometry::Rect {
    tiler_core::geometry::Rect { x, y, w, h }
}

fn limited_observed(
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
    me: &crate::model::ProcessIdentity,
    rect: tiler_core::geometry::Rect,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

/// Two verified existing managed members on one tiled workspace plus their
/// injected observation/tags. Returns the state, output name, and the two
/// member keys. The caller scripts the foreground HWND per case.
fn limited_two_member_state() -> (
    super::TileLoop,
    crate::model::ProcessIdentity,
    String,
    crate::workspace::WindowKey,
    crate::workspace::WindowKey,
) {
    clear_pending_seams();
    let mut state = super::rmax03_adapter_tests::test_state();
    let me = limited_me();
    let output = "mon-limited".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    assert!(state.workspaces.set_tiled(&output, &active, true));
    let key_a = crate::workspace::WindowKey {
        hwnd: 0xA001,
        pid: 7001,
        creation: "creation-a".to_owned(),
    };
    let key_b = crate::workspace::WindowKey {
        hwnd: 0xB002,
        pid: 7002,
        creation: "creation-b".to_owned(),
    };
    for (key, token, pid, creation, tag) in [
        (&key_a, "tok-a", 7001u32, "creation-a", "tag-a"),
        (&key_b, "tok-b", 7002u32, "creation-b", "tag-b"),
    ] {
        assert!(
            state
                .workspaces
                .assign(key.clone(), &output, &active, false)
        );
        state.member_tokens.insert(key.clone(), token.to_owned());
        state.member_identity.insert(
            key.clone(),
            crate::model::ProcessIdentity {
                pid,
                process_creation: creation.to_owned(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state.member_tags.insert(key.clone(), tag.to_owned());
        state
            .member_rects
            .insert(token.to_owned(), limited_frame(10, 10, 400, 300));
    }
    (state, me, output, key_a, key_b)
}

fn limited_two_observed(me: &crate::model::ProcessIdentity) -> Vec<super::ObservedWindow> {
    vec![
        limited_observed(
            0xA001,
            "tok-a",
            7001,
            "creation-a",
            me,
            limited_frame(0, 0, 960, 1040),
        ),
        limited_observed(
            0xB002,
            "tok-b",
            7002,
            "creation-b",
            me,
            limited_frame(960, 0, 960, 1040),
        ),
    ]
}

fn limited_fulls_areas(output: &str) -> (Vec<super::MonitorArea>, Vec<tiler_core::geometry::Rect>) {
    let full = limited_frame(0, 0, 1920, 1080);
    (
        vec![super::MonitorArea {
            device: output.to_owned(),
            work: full,
            full,
        }],
        vec![full],
    )
}

#[test]
fn limited_writable_filter_excludes_foreground_member() {
    let (state, me, output, _key_a, key_b) = limited_two_member_state();
    let observed = limited_two_observed(&me);
    let active = state.workspaces.active_id(&output).expect("active");
    let open = writable_tokens_limited(&state, &output, &active, &observed, 0xB002);
    assert!(open.contains("tok-a"), "other member stays writable");
    assert!(
        !open.contains("tok-b"),
        "foreground member takes no geometry writes"
    );
    let _ = (state, key_b);
    clear_pending_seams();
}

#[test]
fn limited_lane_plans_others_excludes_foreground_without_side_effects() {
    let (mut state, me, output, key_a, key_b) = limited_two_member_state();
    let observed = limited_two_observed(&me);
    let (areas, fulls) = limited_fulls_areas(&output);
    inject_pending_observe(observed, Vec::new(), false);
    inject_pending_tag(0xA001, Some("tag-a".to_owned()));
    inject_pending_tag(0xB002, Some("tag-b".to_owned()));
    inject_pending_suspend(Some(true));
    inject_pending_foreground(Some(0xB002));
    inject_pending_elevated(Some(false));
    let members_before: Vec<crate::workspace::WindowKey> = state
        .workspaces
        .workspace_members(
            &output,
            &state.workspaces.active_id(&output).expect("active"),
        )
        .into_iter()
        .collect();
    let tokens_before = state.member_tokens.clone();
    let attempts_before = state.maximize_admission_attempted.len();
    reconcile_limited_tick(&mut state, &me, &fulls, &areas, 0xB002);
    // Real planning ran for the domain (not the counter-only stub).
    assert_eq!(pending_counters().engine, 1, "limited lane plans once");
    // The stubbed apply records exactly which tokens the plan scoped for
    // writes: the other member, never the foreground.
    let writables = pending_apply_writables();
    assert_eq!(writables.len(), 1, "one domain apply, {writables:?}");
    assert!(writables[0].contains("tok-a"), "{writables:?}");
    assert!(!writables[0].contains("tok-b"), "{writables:?}");
    // No admission, no native restore, no native moves, no focus.
    let members_after: Vec<crate::workspace::WindowKey> = state
        .workspaces
        .workspace_members(
            &output,
            &state.workspaces.active_id(&output).expect("active"),
        )
        .into_iter()
        .collect();
    assert_eq!(members_before, members_after, "fresh roster admits nothing");
    assert_eq!(state.member_tokens, tokens_before, "no token churn");
    assert_eq!(
        state.maximize_admission_attempted.len(),
        attempts_before,
        "no native un-maximize attempt"
    );
    assert!(
        pending_native_transfers().is_empty(),
        "no float/overlay/minimized/hidden moves"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "automatic focus never actuates"
    );
    assert_eq!(pending_counters().reconcile, 0, "full reconcile never runs");
    let _ = (key_a, key_b);
    clear_pending_seams();
}

#[test]
fn limited_lane_refuses_without_foreground() {
    let (mut state, me, output, _key_a, _key_b) = limited_two_member_state();
    let observed = limited_two_observed(&me);
    let (areas, fulls) = limited_fulls_areas(&output);
    inject_pending_observe(observed, Vec::new(), false);
    inject_pending_suspend(Some(true));
    inject_pending_foreground(None);
    inject_pending_elevated(Some(false));
    reconcile_limited_tick(&mut state, &me, &fulls, &areas, 0);
    assert_eq!(pending_counters().engine, 0, "unknown FG plans nothing");
    assert!(
        pending_apply_writables().is_empty(),
        "unknown FG writes nothing"
    );
    clear_pending_seams();
}

#[test]
fn limited_tick_via_run_loop_reconciles_while_unknown_suspends() {
    for (fg, expect_engine) in [(Some(0xB002u64), 1), (None, 0)] {
        let (mut state, me, output, _key_a, _key_b) = limited_two_member_state();
        let observed = limited_two_observed(&me);
        let (mut areas, _fulls) = limited_fulls_areas(&output);
        inject_pending_observe(observed, Vec::new(), false);
        inject_pending_tag(0xA001, Some("tag-a".to_owned()));
        inject_pending_tag(0xB002, Some("tag-b".to_owned()));
        inject_pending_suspend(Some(true));
        inject_pending_foreground(fg);
        inject_pending_elevated(Some(false));
        let tmp = std::env::temp_dir().join(format!(
            "tiler-limited-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
        let dir = tmp.clone();
        let mut monitor = areas[0].clone();
        let monitor_count = &mut 1usize;
        let log_path = std::path::PathBuf::from("NUL");
        let mut guard = super::OwnerWorkGuard::begin(false, &log_path, 0, state.tick, None);
        let ow_counts = super::OwnerWaitCounts::default();
        let flow = run_active_tick(
            &mut state,
            &me,
            &dir,
            &store,
            &mut areas,
            &mut monitor,
            monitor_count,
            std::time::Instant::now(),
            false,
            false,
            true,
            &log_path,
            &mut guard,
            &ow_counts,
            false,
            &mut false,
            &mut None,
            0,
            0,
            0,
            0,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &mut None,
        )
        .expect("limited tick runs");
        assert!(
            matches!(flow, TickFlow::Suspended),
            "both shapes stay on the suspended flow"
        );
        assert!(state.suspended, "snap stays suspended under fullscreen");
        assert_eq!(
            pending_counters().engine,
            expect_engine,
            "limited fg={fg:?} plans, unknown plans nothing"
        );
        if expect_engine == 1 {
            let writables = pending_apply_writables();
            assert_eq!(writables.len(), 1, "{writables:?}");
            assert!(writables[0].contains("tok-a"), "{writables:?}");
            assert!(!writables[0].contains("tok-b"), "{writables:?}");
        } else {
            assert!(pending_apply_writables().is_empty());
        }
        assert_eq!(
            pending_counters().focus,
            0,
            "automatic focus never actuates"
        );
        assert!(
            pending_native_transfers().is_empty(),
            "no native moves in either lane"
        );
        clear_pending_seams();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
