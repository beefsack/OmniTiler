// Production-preamble adoption through a real `TileLoop` against an
// owned message-only window: real `SetProp` markers, real `GetProp`
// reads, real Engine commits. Message-only windows are invisible,
// owned by the test process, and destroyed at test end; the log path
// points at NUL so no file is created. No other window is touched.
use super::adopt_restart_markers;
use super::rmax03_adapter_tests::test_state;
use crate::model::ProcessIdentity;
use crate::workspace::WindowKey;
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::DomainKey;

const ADOPT_TEST_CLASS: &str = "OmniTilerRestartAdoptTest";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe extern "system" fn adopt_wndproc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wp: windows_sys::Win32::Foundation::WPARAM,
    lp: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wp, lp) }
}

struct AdoptWindow {
    hwnd: u64,
}

impl AdoptWindow {
    fn create() -> AdoptWindow {
        use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, HWND_MESSAGE, RegisterClassW, WNDCLASSW,
        };
        let class_w = wide(ADOPT_TEST_CLASS);
        let title_w = wide("restart-adopt-test");
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.lpfnWndProc = Some(adopt_wndproc);
        cls.hInstance = hinst;
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        assert!(
            atom != 0 || unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS,
            "test class registers"
        );
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_w.as_ptr(),
                title_w.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            )
        };
        assert!(!hwnd.is_null(), "message-only test window creates");
        AdoptWindow {
            hwnd: hwnd as usize as u64,
        }
    }
}

impl Drop for AdoptWindow {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
        unsafe {
            DestroyWindow(self.hwnd as usize as HWND);
        }
    }
}

fn fake_me() -> ProcessIdentity {
    ProcessIdentity {
        pid: std::process::id(),
        process_creation: "create-restart-adopt".to_owned(),
        user_sid: "S-1-5-21-restart".to_owned(),
        session_id: 1,
        exe_path: "C:\\test\\app.exe".to_owned(),
    }
}

fn bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }
}

fn frame() -> Rect {
    Rect {
        x: 10,
        y: 10,
        w: 400,
        h: 300,
    }
}

fn observed(hwnd: u64, token: &str) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: bounds(),
        visible: frame(),
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid: std::process::id(),
            process_creation: "create-restart-adopt".to_owned(),
            exe_path: "C:\\test\\app.exe".to_owned(),
            user_sid: "S-1-5-21-restart".to_owned(),
            session_id: 1,
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

/// Seed admitted membership for one owned window and return its key,
/// token, output, and active workspace. Mirrors the admission preamble
/// tables (tokens, rects, identity, tags, workspace) without geometry.
fn seed_member(
    state: &mut super::TileLoop,
    hwnd: u64,
    tag: &str,
) -> (WindowKey, String, String, String) {
    let me = fake_me();
    let key = WindowKey {
        hwnd,
        pid: me.pid,
        creation: me.process_creation.clone(),
    };
    let token = state.tokens.token_for(hwnd, &key.creation);
    let output = "mon-restart".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    assert!(
        state
            .workspaces
            .assign(key.clone(), &output, &active, false)
    );
    state.member_tokens.insert(key.clone(), token.clone());
    state.member_rects.insert(token.clone(), frame());
    state.member_identity.insert(key.clone(), me);
    state.member_tags.insert(key.clone(), tag.to_owned());
    (key, token, output, active)
}

fn areas_for(output: &str) -> Vec<super::MonitorArea> {
    vec![super::MonitorArea {
        device: output.to_owned(),
        work: bounds(),
        full: bounds(),
    }]
}

fn is_float(state: &super::TileLoop, output: &str, ws: &str, token: &str) -> bool {
    let key = DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
    };
    state
        .engine
        .session(&key)
        .is_some_and(|s| s.is_exception(&WindowId(token.to_owned())))
}

#[test]
fn sticky_markers_readopt_through_real_tileloop_twice() {
    // Both pre-sticky origins re-adopt as sticky through the production
    // preamble with real markers, and a second fresh owner re-adopts
    // again off the kept marker. No post-restart un-stick outcome set.
    for prior in [false, true] {
        let win = AdoptWindow::create();
        let pid = std::process::id();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
            .expect("member tag installs on the owned window");
        crate::product_hide::sys::install_sticky_marker(win.hwnd, prior)
            .expect("sticky marker installs on the owned window");
        for restart in 1..=2 {
            let mut state = test_state();
            state.log_path = std::path::PathBuf::from("NUL");
            let (key, token, output, active) = seed_member(&mut state, win.hwnd, &tag);
            let observed = vec![observed(win.hwnd, &token)];
            let mut hint_cx = super::HintCx::new();
            adopt_restart_markers(
                &mut state,
                &fake_me(),
                &observed,
                &[],
                &areas_for(&output),
                &mut hint_cx,
            );
            assert_eq!(
                state.sticky.get(&key),
                Some(&prior),
                "prior {prior} restart {restart}: sticky origin re-adopted"
            );
            assert!(
                is_float(&state, &output, &active, &token),
                "prior {prior} restart {restart}: Engine carries the float"
            );
            assert!(
                crate::product_hide::sys::peek_sticky_raw(win.hwnd)
                    == crate::workspace_owner::MarkerSlot::Value(
                        crate::tiling::sticky_marker_value(prior)
                    ),
                "prior {prior} restart {restart}: durable marker kept"
            );
        }
    }
}

#[test]
fn tile_override_marker_hydrates_override_without_writes_or_float() {
    // D7 through the production preamble with a real marker: a surviving
    // tile-override marker hydrates the runtime override set with the
    // marker kept, stages no Engine commit, floats nothing, and takes
    // no sticky entry. A second fresh owner re-adopts off the kept
    // marker. Unmatched records never reach the set (fences above).
    let win = AdoptWindow::create();
    let pid = std::process::id();
    let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
        .expect("member tag installs on the owned window");
    crate::product_hide::sys::install_tile_override_marker(win.hwnd)
        .expect("tile marker installs on the owned window");
    for restart in 1..=2 {
        let mut state = test_state();
        state.log_path = std::path::PathBuf::from("NUL");
        let (key, token, output, active) = seed_member(&mut state, win.hwnd, &tag);
        let observed = vec![observed(win.hwnd, &token)];
        let mut hint_cx = super::HintCx::new();
        adopt_restart_markers(
            &mut state,
            &fake_me(),
            &observed,
            &[],
            &areas_for(&output),
            &mut hint_cx,
        );
        assert!(
            state.fixed_tile_overrides.contains(&key),
            "restart {restart}: override hydrated into the runtime set"
        );
        assert!(
            !state.floated.contains(&key),
            "restart {restart}: override never floats"
        );
        assert!(
            !state.sticky.contains_key(&key),
            "restart {restart}: override takes no sticky entry"
        );
        assert!(
            !is_float(&state, &output, &active, &token),
            "restart {restart}: adoption stages no Engine commit"
        );
        assert!(
            crate::product_hide::sys::read_tile_override_marker(win.hwnd),
            "restart {restart}: durable marker kept for later restarts"
        );
    }
}

/// Seed admitted membership for one owned window with its REAL process
/// identity (not the fake adoption identity): the held fence compares
/// live identity, sid/session, exe, and integrity against these tables,
/// so only the true owner passes.
fn seed_real_member(
    state: &mut super::TileLoop,
    hwnd: u64,
    tag: &str,
    real: &ProcessIdentity,
) -> (WindowKey, String, String, String) {
    let key = WindowKey {
        hwnd,
        pid: real.pid,
        creation: real.process_creation.clone(),
    };
    let token = state.tokens.token_for(hwnd, &key.creation);
    let output = "mon-restart".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    assert!(
        state
            .workspaces
            .assign(key.clone(), &output, &active, false)
    );
    state.member_tokens.insert(key.clone(), token.clone());
    state.member_rects.insert(token.clone(), frame());
    state.member_identity.insert(key.clone(), real.clone());
    state.member_tags.insert(key.clone(), tag.to_owned());
    (key, token, output, active)
}

fn retained_max(key: &WindowKey, token: &str) -> super::RetainedRow {
    super::RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(frame()),
        maximized: true,
        fullscreen: false,
        facts: Some(crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: true,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        }),
    }
}

fn runner_is_medium() -> bool {
    // Own-process windows inherit the runner integrity level, and the
    // held fence refuses non-medium processes before any marker read.
    // Medium runners hydrate; elevated runners stop at the gate (the
    // existing skip contract: refusal-shaped emptiness, never success).
    crate::native::current_integrity_level()
        .ok()
        .is_some_and(crate::lifecycle::is_medium_rid)
}

#[test]
fn retained_max_marker_hydrates_durable_before_auto_and_clear() {
    // D7 retained pass on a real owned message-only window presented as
    // a retained maximized member with hint loss: the production held
    // install plus readback persists the marker success-only, then the
    // production preamble hydrates durable+runtime with no Engine
    // commit, no native frame/focus/style writes, no D4 latch, and no
    // clear under both born-maximized modes. Controls: corrupt value,
    // tag mismatch, destroyed window, settled removal, and membership
    // prune each hydrate nothing. Message-only windows are invisible
    // and destroyed at test end; no other window is touched.
    use crate::settings::{BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, LiveSettings, Settings};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetParent, GetWindowLongW, SetPropW,
    };
    let real = crate::native::current_identity().expect("current identity reads");
    let medium = runner_is_medium();
    let win = AdoptWindow::create();
    let tag = crate::product_hide::sys::install_member_tag(win.hwnd, real.pid)
        .expect("member tag installs on the owned window");
    let style_before = unsafe { GetWindowLongW(win.hwnd as usize as HWND, GWL_EXSTYLE) };
    let parent_before = unsafe { GetParent(win.hwnd as usize as HWND) };
    for born in [BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_OVERLAY] {
        let mut state = test_state();
        let mut settings = Settings::default();
        settings.core.born_maximized = born.to_owned();
        state.settings_live = Some(LiveSettings::fresh(settings, None));
        let (key, token, output, active) = seed_real_member(&mut state, win.hwnd, &tag, &real);
        // Success-only production install through the held wrapper.
        let installed = super::install_tile_override_mark_held(&state, &real, &key);
        let mut hint_cx = super::HintCx::new();
        // Hint loss: no injection anywhere, so every fresh probe reads
        // unknown; hydration takes no hints at all.
        super::clear_test_hints();
        // Maximized members never appear in `observed`.
        let retained = vec![retained_max(&key, &token)];
        super::adopt_restart_markers(
            &mut state,
            &real,
            &[],
            &retained,
            &areas_for(&output),
            &mut hint_cx,
        );
        if !medium {
            assert!(
                !installed,
                "born {born}: elevated runner refuses the install"
            );
            assert!(
                !state.fixed_tile_overrides.contains(&key)
                    && !state.fixed_origin_overrides.contains(&key),
                "born {born}: elevated runner hydrates nothing"
            );
            continue;
        }
        assert!(installed, "born {born}: held install reads back");
        assert!(
            crate::product_hide::sys::read_tile_override_marker(win.hwnd),
            "born {born}: marker present"
        );
        assert!(
            state.fixed_tile_overrides.contains(&key),
            "born {born}: runtime hydrated"
        );
        assert!(
            state.fixed_origin_overrides.contains(&key),
            "born {born}: durable hydrated"
        );
        assert!(
            state.fixed_auto_seen.is_empty(),
            "born {born}: no automatic origin acquired"
        );
        let domain = DomainKey {
            output: OutputId(output.clone()),
            workspace: WorkspaceId(active.clone()),
        };
        assert!(
            state.engine.session(&domain).is_none(),
            "born {born}: hydration stages no Engine commit"
        );
        assert_eq!(
            unsafe { GetWindowLongW(win.hwnd as usize as HWND, GWL_EXSTYLE) },
            style_before,
            "born {born}: no native restyle"
        );
        assert_eq!(
            unsafe { GetParent(win.hwnd as usize as HWND) },
            parent_before,
            "born {born}: no reparenting"
        );
        let cx = super::HintCx::new();
        assert!(
            !super::admission_clear_planned(&state, &retained[0], &cx),
            "born {born}: durable never plans CLEAR"
        );
        let rows = super::assemble_domain_rows(
            &mut state,
            &output,
            &active,
            &[],
            &retained,
            "adopt-positive",
            "adopt",
            &mut hint_cx,
        )
        .expect("rows assemble");
        assert!(
            rows.iter().any(|row| row.token == token && !row.floating),
            "born {born}: override rides the tiled lane"
        );
        assert!(
            state.member_rects.contains_key(&token),
            "born {born}: reserved maximized slot stands"
        );
        // Settled removal through the production wrapper clears the
        // marker; membership prune drops both runtime lanes.
        super::remove_tile_override_mark_best_effort(
            &state,
            &real,
            &key,
            &state.log_path.clone(),
            &token,
        );
        assert!(
            !crate::product_hide::sys::read_tile_override_marker(win.hwnd),
            "born {born}: settled removal clears the marker"
        );
        state.fixed_tile_overrides.remove(&key);
        state.fixed_origin_overrides.remove(&key);
        super::drop_member_state(&mut state, &key);
        assert!(
            !state.fixed_tile_overrides.contains(&key)
                && !state.fixed_origin_overrides.contains(&key),
            "born {born}: prune drops both lanes"
        );
        // Re-install for the next born mode (marker removal above).
        super::install_tile_override_mark_held(&state, &real, &key);
    }
    if !medium {
        return;
    }
    // Corrupt value on our property name: diagnosed, no intent.
    {
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain([0]).collect()
        }
        let win = AdoptWindow::create();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, real.pid)
            .expect("member tag installs");
        let name = wide(crate::model::TILE_OVERRIDE_PROP);
        let ok = unsafe { SetPropW(win.hwnd as usize as HWND, name.as_ptr(), 99usize as _) };
        assert_ne!(ok, 0, "foreign value plants on the owned window");
        let mut state = test_state();
        let (key, token, output, _) = seed_real_member(&mut state, win.hwnd, &tag, &real);
        let retained = vec![retained_max(&key, &token)];
        let mut hint_cx = super::HintCx::new();
        super::adopt_restart_markers(
            &mut state,
            &real,
            &[],
            &retained,
            &areas_for(&output),
            &mut hint_cx,
        );
        assert!(
            !state.fixed_tile_overrides.contains(&key)
                && !state.fixed_origin_overrides.contains(&key),
            "corrupt plants no override"
        );
    }
    // Tag mismatch: the held fence refuses without the install tag.
    {
        let win = AdoptWindow::create();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, real.pid)
            .expect("member tag installs");
        crate::product_hide::sys::install_tile_override_marker(win.hwnd)
            .expect("tile marker installs");
        let mut state = test_state();
        let (key, token, output, _) =
            seed_real_member(&mut state, win.hwnd, "deadbeefdeadbeef", &real);
        let retained = vec![retained_max(&key, &token)];
        let mut hint_cx = super::HintCx::new();
        super::adopt_restart_markers(
            &mut state,
            &real,
            &[],
            &retained,
            &areas_for(&output),
            &mut hint_cx,
        );
        assert!(
            !state.fixed_tile_overrides.contains(&key)
                && !state.fixed_origin_overrides.contains(&key),
            "tag mismatch hydrates nothing"
        );
        let _ = tag;
    }
    // Destroyed window: close refuses adoption with no state change.
    {
        let win = AdoptWindow::create();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, real.pid)
            .expect("member tag installs");
        crate::product_hide::sys::install_tile_override_marker(win.hwnd)
            .expect("tile marker installs");
        let hwnd = win.hwnd;
        drop(win);
        let mut state = test_state();
        let (key, token, output, _) = seed_real_member(&mut state, hwnd, &tag, &real);
        let retained = vec![retained_max(&key, &token)];
        let mut hint_cx = super::HintCx::new();
        super::adopt_restart_markers(
            &mut state,
            &real,
            &[],
            &retained,
            &areas_for(&output),
            &mut hint_cx,
        );
        assert!(
            !state.fixed_tile_overrides.contains(&key)
                && !state.fixed_origin_overrides.contains(&key),
            "destroyed window hydrates nothing"
        );
    }
}

#[test]
fn float_marker_readopts_through_real_tileloop() {
    // An intentional-float marker re-adopts as an ordinary float with
    // the marker kept and no sticky entry.
    let win = AdoptWindow::create();
    let pid = std::process::id();
    let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
        .expect("member tag installs on the owned window");
    crate::product_hide::sys::install_float_intent_marker(win.hwnd)
        .expect("float marker installs on the owned window");
    let mut state = test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let (key, token, output, active) = seed_member(&mut state, win.hwnd, &tag);
    let observed = vec![observed(win.hwnd, &token)];
    let mut hint_cx = super::HintCx::new();
    adopt_restart_markers(
        &mut state,
        &fake_me(),
        &observed,
        &[],
        &areas_for(&output),
        &mut hint_cx,
    );
    assert!(
        state.floated.contains(&key),
        "float intent re-adopted into the runtime set"
    );
    assert!(
        !state.sticky.contains_key(&key),
        "ordinary float takes no sticky entry"
    );
    assert!(
        is_float(&state, &output, &active, &token),
        "Engine carries the re-adopted float"
    );
    assert!(
        crate::product_hide::sys::read_float_intent_marker(win.hwnd),
        "durable float marker kept for later restarts"
    );
}

#[test]
fn mixed_markers_readopt_through_real_tileloop_in_any_order() {
    // Live item 8 shape: two sticky markers (both pre-sticky origins),
    // one ordinary float marker, and one unmarked tile in a single
    // domain. The production preamble (sticky pass then float pass) must
    // hydrate all three before any tiling, in either observed order.
    // Log goes to a temp file so a candidate rejection shows its true
    // correlated cause instead of a silent rollback.
    for reverse in [false, true] {
        let a = AdoptWindow::create();
        let b = AdoptWindow::create();
        let c = AdoptWindow::create();
        let d = AdoptWindow::create();
        let pid = std::process::id();
        let install = |win: &AdoptWindow| {
            crate::product_hide::sys::install_member_tag(win.hwnd, pid)
                .expect("member tag installs on the owned window")
        };
        let (tag_a, tag_b, tag_c, tag_d) = (install(&a), install(&b), install(&c), install(&d));
        crate::product_hide::sys::install_sticky_marker(a.hwnd, false)
            .expect("sticky marker installs");
        crate::product_hide::sys::install_sticky_marker(b.hwnd, true)
            .expect("sticky marker installs");
        crate::product_hide::sys::install_float_intent_marker(c.hwnd)
            .expect("float marker installs");
        let mut state = test_state();
        let log_path = std::env::temp_dir().join(format!(
            "tiler-windows-mixed-adopt-{}-{}.log",
            std::process::id(),
            reverse as u8
        ));
        let _ = std::fs::remove_file(&log_path);
        state.log_path = log_path.clone();
        let (key_a, token_a, output, active) = seed_member(&mut state, a.hwnd, &tag_a);
        let (key_b, token_b, _, _) = seed_member(&mut state, b.hwnd, &tag_b);
        let (key_c, token_c, _, _) = seed_member(&mut state, c.hwnd, &tag_c);
        let (key_d, token_d, _, _) = seed_member(&mut state, d.hwnd, &tag_d);
        let mut ordered = vec![
            observed(a.hwnd, &token_a),
            observed(b.hwnd, &token_b),
            observed(c.hwnd, &token_c),
            observed(d.hwnd, &token_d),
        ];
        if reverse {
            ordered.reverse();
        }
        let mut hint_cx = super::HintCx::new();
        adopt_restart_markers(
            &mut state,
            &fake_me(),
            &ordered,
            &[],
            &areas_for(&output),
            &mut hint_cx,
        );
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        let _ = std::fs::remove_file(&log_path);
        assert_eq!(
            state.sticky.get(&key_a),
            Some(&false),
            "reverse {reverse}: ex-tiled sticky re-adopts; log:\n{log}"
        );
        assert_eq!(
            state.sticky.get(&key_b),
            Some(&true),
            "reverse {reverse}: ex-float sticky re-adopts; log:\n{log}"
        );
        assert!(
            state.floated.contains(&key_c),
            "reverse {reverse}: ordinary float re-adopts; log:\n{log}"
        );
        assert!(
            !state.sticky.contains_key(&key_d) && !state.floated.contains(&key_d),
            "reverse {reverse}: unmarked tile stays tiled; log:\n{log}"
        );
        for (token, what) in [
            (&token_a, "sticky-a"),
            (&token_b, "sticky-b"),
            (&token_c, "float-c"),
        ] {
            assert!(
                is_float(&state, &output, &active, token),
                "reverse {reverse}: Engine carries {what}; log:\n{log}"
            );
        }
        assert!(
            !is_float(&state, &output, &active, &token_d),
            "reverse {reverse}: Engine keeps the tile tiled; log:\n{log}"
        );
    }
}

#[test]
fn marker_install_remove_roundtrip_on_owned_window() {
    // Actual persistence native ops: install reads back, settled removal
    // verifies absence, and a wrong sticky value refuses without
    // touching the live marker.
    let win = AdoptWindow::create();
    assert!(
        !crate::product_hide::sys::read_float_intent_marker(win.hwnd),
        "fresh window carries no float intent"
    );
    crate::product_hide::sys::install_float_intent_marker(win.hwnd)
        .expect("float marker installs on the owned window");
    assert!(crate::product_hide::sys::read_float_intent_marker(win.hwnd));
    crate::product_hide::sys::remove_float_intent_marker(win.hwnd)
        .expect("settled float removal verifies");
    assert!(
        !crate::product_hide::sys::read_float_intent_marker(win.hwnd),
        "settled removal clears the float marker"
    );
    crate::product_hide::sys::install_sticky_marker(win.hwnd, true)
        .expect("sticky marker installs on the owned window");
    assert!(
        !crate::product_hide::sys::remove_sticky_marker(win.hwnd, false).expect("readable"),
        "wrong pre-sticky value refuses without touching"
    );
    assert_eq!(
        crate::product_hide::sys::read_sticky_marker(win.hwnd),
        Some(true),
        "refused removal keeps the live marker"
    );
    assert!(
        crate::product_hide::sys::remove_sticky_marker(win.hwnd, true).expect("readable"),
        "settled removal with the live value verifies"
    );
    assert_eq!(
        crate::product_hide::sys::read_sticky_marker(win.hwnd),
        None,
        "settled removal clears the sticky marker"
    );
}

#[test]
fn absent_markers_read_silent_never_unreadable() {
    // Never-installed marker names read silent-absent on a real owned
    // window: `GetPropW` documents NULL for a missing string with no
    // last-error promise, and absent reads report nonzero errors here,
    // so NULL must never classify as unreadable (else every unmarked
    // window logs marker-unreadable every tick).
    let win = AdoptWindow::create();
    for peek in [
        crate::product_hide::sys::peek_sticky_raw(win.hwnd),
        crate::product_hide::sys::peek_float_intent_raw(win.hwnd),
        crate::product_hide::sys::peek_tile_override_raw(win.hwnd),
    ] {
        assert_eq!(peek, crate::workspace_owner::MarkerSlot::Absent);
    }
}

#[test]
fn corrupt_marker_adopts_nothing_without_side_effects() {
    // A foreign value on our property name classifies corrupt through
    // the production preamble: diagnosed, no intent, no state change.
    use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    let win = AdoptWindow::create();
    let pid = std::process::id();
    let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
        .expect("member tag installs on the owned window");
    let name = wide(crate::model::FLOAT_INTENT_PROP);
    let ok = unsafe {
        SetPropW(
            win.hwnd as usize as windows_sys::Win32::Foundation::HWND,
            name.as_ptr(),
            99usize as _,
        )
    };
    assert_ne!(ok, 0, "foreign value plants on the owned window");
    let mut state = test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let (key, token, output, _active) = seed_member(&mut state, win.hwnd, &tag);
    let observed = vec![observed(win.hwnd, &token)];
    let mut hint_cx = super::HintCx::new();
    adopt_restart_markers(
        &mut state,
        &fake_me(),
        &observed,
        &[],
        &areas_for(&output),
        &mut hint_cx,
    );
    assert!(!state.sticky.contains_key(&key), "corrupt never sticks");
    assert!(!state.floated.contains(&key), "corrupt never floats");
}

#[test]
fn absent_markers_adopt_nothing_without_side_effects() {
    // A fabricated HWND exercises only failing reads: no adoption, no
    // state change, no writes, no panic.
    let mut state = test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let hwnd = 0x00BEEF42u64;
    let (key, token, output, _active) = seed_member(&mut state, hwnd, "9f2c41aa07bd33e0");
    let observed = vec![observed(hwnd, &token)];
    let mut hint_cx = super::HintCx::new();
    adopt_restart_markers(
        &mut state,
        &fake_me(),
        &observed,
        &[],
        &areas_for(&output),
        &mut hint_cx,
    );
    assert!(
        !state.sticky.contains_key(&key),
        "no sticky without a marker"
    );
    assert!(!state.floated.contains(&key), "no float without a marker");
    assert!(
        !state.float_rects.contains_key(&token),
        "no float frame without adoption"
    );
}
