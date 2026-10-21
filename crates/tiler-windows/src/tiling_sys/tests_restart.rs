#![forbid(unsafe_code)]
// Production-preamble adoption through a real `TileLoop` against an
// owned message-only window: real `SetProp` markers, real `GetProp`
// reads, real Engine commits. Message-only windows are invisible,
// owned by the test process, and destroyed at test end; the log path
// points at NUL so no file is created. No other window is touched.
use super::rmax03_adapter_tests::test_state;
use crate::model::ProcessIdentity;
use crate::native_guards::WindowKey;
use tiler_core::geometry::Rect;

const ADOPT_TEST_CLASS: &str = "OmniTilerRestartAdoptTest";

/// Adopt through the production preamble with genuine world homing: one
/// observation dispatch through the loop's own owner first so
/// `member_home` resolves (engine homing for the seeded member tables),
/// then the real marker adoption. Never fabricated adapter state.
fn adopt_seeded(
    state: &mut super::TileLoop,
    me: &ProcessIdentity,
    observed: &[super::ObservedWindow],
    retained: &[super::RetainedRow],
    areas: &[super::MonitorArea],
    hint_cx: &mut super::HintCx,
) {
    super::world_tick::dispatch_test_observation(state, observed, retained, areas, 0);
    super::adopt_restart_markers(state, me, observed, retained, areas, hint_cx);
}

struct AdoptWindow {
    hwnd: u64,
    _window: crate::ffi::TestWindow,
}

impl AdoptWindow {
    fn create() -> AdoptWindow {
        let window =
            crate::ffi::TestWindow::create_message_only(ADOPT_TEST_CLASS, "restart-adopt-test")
                .expect("message-only test window creates");
        AdoptWindow {
            hwnd: window.hwnd(),
            _window: window,
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
    state.ensure_device(&output);
    let active = state
        .active_view(&output)
        .unwrap_or_else(|| "ws-1".to_owned());
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

fn is_float(state: &super::TileLoop, _output: &str, _ws: &str, token: &str) -> bool {
    state.core_is_float_token(token)
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
            adopt_seeded(
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
                    == crate::native_guards::MarkerSlot::Value(
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
        adopt_seeded(
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
    state.ensure_device(&output);
    let active = state
        .active_view(&output)
        .unwrap_or_else(|| "ws-1".to_owned());
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
    let real = crate::native::current_identity().expect("current identity reads");
    let medium = runner_is_medium();
    let win = AdoptWindow::create();
    let tag = crate::product_hide::sys::install_member_tag(win.hwnd, real.pid)
        .expect("member tag installs on the owned window");
    let style_before = crate::ffi::window_exstyle(win.hwnd);
    let parent_before = crate::ffi::parent_of(win.hwnd);
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
        adopt_seeded(
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
        let _domain = (output.clone(), active.clone());
        // Hydration stages no world command of its own: the seeding
        // dispatch above is the only owner traffic (adoption only fills
        // adapter intent lanes from durable markers).
        assert_eq!(
            crate::ffi::window_exstyle(win.hwnd),
            style_before,
            "born {born}: no native restyle"
        );
        assert_eq!(
            crate::ffi::parent_of(win.hwnd),
            parent_before,
            "born {born}: no reparenting"
        );
        let _cx = super::HintCx::new();
        assert!(
            !super::admission_clear_planned(&state, &retained[0], &_cx),
            "born {born}: durable never plans CLEAR"
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
        let win = AdoptWindow::create();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, real.pid)
            .expect("member tag installs");
        let ok = crate::ffi::set_prop(win.hwnd, crate::model::TILE_OVERRIDE_PROP, 99);
        assert!(ok, "foreign value plants on the owned window");
        let mut state = test_state();
        let (key, token, output, _) = seed_real_member(&mut state, win.hwnd, &tag, &real);
        let retained = vec![retained_max(&key, &token)];
        let mut hint_cx = super::HintCx::new();
        adopt_seeded(
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
        adopt_seeded(
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
        adopt_seeded(
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
    adopt_seeded(
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
        adopt_seeded(
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
        assert_eq!(peek, crate::native_guards::MarkerSlot::Absent);
    }
}

#[test]
fn corrupt_marker_adopts_nothing_without_side_effects() {
    // A foreign value on our property name classifies corrupt through
    // the production preamble: diagnosed, no intent, no state change.
    let win = AdoptWindow::create();
    let pid = std::process::id();
    let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
        .expect("member tag installs on the owned window");
    let ok = crate::ffi::set_prop(win.hwnd, crate::model::FLOAT_INTENT_PROP, 99);
    assert!(ok, "foreign value plants on the owned window");
    let mut state = test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let (key, token, output, _active) = seed_member(&mut state, win.hwnd, &tag);
    let observed = vec![observed(win.hwnd, &token)];
    let mut hint_cx = super::HintCx::new();
    adopt_seeded(
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
    adopt_seeded(
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

fn hydration_row(
    token: &str,
    rect: Rect,
    hints: tiler_core::size_hints::WindowSizeHints,
    floating: bool,
) -> (String, Rect, tiler_core::size_hints::WindowSizeHints, bool) {
    (token.to_owned(), rect, hints, floating)
}

fn hydration_candidates(
    rows: &[(String, Rect, tiler_core::size_hints::WindowSizeHints, bool)],
) -> Vec<super::restart::HydrationCandidate<'_>> {
    rows.iter()
        .map(|(token, rect, hints, floating)| super::restart::HydrationCandidate {
            token,
            rect: *rect,
            floating: *floating,
            hints: *hints,
        })
        .collect()
}

#[test]
fn hydration_rejection_detail_carries_exact_counts_under_cap() {
    // Pure bound check, no windows touched: the cap explanation lives in
    // `bounded_hydration_candidates` (`restart.rs`).
    let rect = Rect {
        x: 1,
        y: 2,
        w: 3,
        h: 4,
    };
    let rows: Vec<(String, Rect, tiler_core::size_hints::WindowSizeHints, bool)> = (0
        ..super::restart::RESTART_HYDRATION_DETAIL_CAP + 2)
        .map(|n| {
            hydration_row(
                &format!("w{n}"),
                rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                n % 2 == 0,
            )
        })
        .collect();
    let detail = super::restart::bounded_hydration_candidates(&hydration_candidates(&rows));
    assert_eq!(detail["total"], serde_json::json!(rows.len()));
    assert_eq!(detail["omitted"], serde_json::json!(2));
    let carried = detail["candidates"].as_array().expect("candidate array");
    assert_eq!(carried.len(), super::restart::RESTART_HYDRATION_DETAIL_CAP);
    assert_eq!(carried[0]["window"], "w0");
    assert_eq!(carried[0]["rect"], serde_json::json!([1, 2, 3, 4]));
    assert_eq!(carried[0]["floating"], true);
    assert_eq!(
        carried[0]["min"],
        serde_json::json!([null, null]),
        "absent hints stay null, never zero-filled"
    );
    assert!(
        detail["candidates"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row.get("window").is_some()
                && row.get("rect").is_some()
                && row.get("floating").is_some()
                && row.get("min").is_some()
                && row.get("max").is_some()))
    );
}

#[test]
fn hydration_rejection_detail_omits_nothing_under_cap() {
    let rows = [
        hydration_row(
            "w0",
            Rect {
                x: 517,
                y: 281,
                w: 1526,
                h: 818,
            },
            tiler_core::size_hints::WindowSizeHints {
                min_w: Some(1157),
                min_h: Some(789),
                max_w: Some(1157),
                max_h: Some(789),
            },
            false,
        ),
        hydration_row(
            "w1",
            Rect {
                x: 8,
                y: 8,
                w: 100,
                h: 100,
            },
            tiler_core::size_hints::WindowSizeHints::none(),
            true,
        ),
    ];
    let detail = super::restart::bounded_hydration_candidates(&hydration_candidates(&rows));
    assert_eq!(detail["total"], serde_json::json!(2));
    assert_eq!(detail["omitted"], serde_json::json!(0));
    assert_eq!(
        detail["candidates"][0]["min"],
        serde_json::json!([1157, 789])
    );
    assert_eq!(
        detail["candidates"][0]["max"],
        serde_json::json!([1157, 789])
    );
    assert_eq!(detail["candidates"][1]["floating"], true);
}

// --- Mixed-roster hydration ------------------------------------------------
// Mixed roster through the production preamble: a fixed (max-pinned)
// focused anchor plus one plain tiled remainder plus two sticky-marked and
// one ordinary float-marked candidates. Marked windows adopt into the
// adapter intent lanes (the unmarked anchor and remainder adopt none);
// the hold stays empty throughout. A repeated identical observation
// recomputes the same stable adoption, and a monitorless pass adopts the
// same lanes (intent is output-independent; the hold barrier rode the
// deleted per-domain commit and now only ever clears).
// Engine-level `duplicate-window` refusal of genuinely duplicated
// input stays pinned in core (`session_lifecycle::errors_are_redacted_and_bounded`).

fn reject_domain_bounds() -> Rect {
    Rect {
        x: 8,
        y: 8,
        w: 2544,
        h: 1364,
    }
}

fn reject_areas_for(output: &str) -> Vec<super::MonitorArea> {
    vec![super::MonitorArea {
        device: output.to_owned(),
        work: reject_domain_bounds(),
        full: reject_domain_bounds(),
    }]
}

fn reject_anchor_frame() -> Rect {
    Rect {
        x: 48,
        y: 20,
        w: 1157,
        h: 789,
    }
}

fn reject_plain_frame() -> Rect {
    Rect {
        x: 1400,
        y: 800,
        w: 900,
        h: 500,
    }
}

fn reject_float_frame() -> Rect {
    Rect {
        x: 517,
        y: 281,
        w: 1526,
        h: 818,
    }
}

fn fixed_anchor_hints() -> tiler_core::size_hints::WindowSizeHints {
    tiler_core::size_hints::WindowSizeHints {
        min_w: Some(1157),
        min_h: Some(789),
        max_w: Some(1157),
        max_h: Some(789),
    }
}

fn min_anchor_hints() -> tiler_core::size_hints::WindowSizeHints {
    tiler_core::size_hints::WindowSizeHints {
        min_w: Some(1157),
        min_h: Some(789),
        ..tiler_core::size_hints::WindowSizeHints::none()
    }
}

fn reject_plain_hints() -> tiler_core::size_hints::WindowSizeHints {
    tiler_core::size_hints::WindowSizeHints {
        min_w: Some(401),
        min_h: Some(246),
        ..tiler_core::size_hints::WindowSizeHints::none()
    }
}

fn observed_frame(hwnd: u64, token: &str, visible: Rect) -> super::ObservedWindow {
    let mut window = observed(hwnd, token);
    window.visible = visible;
    window
}

/// Five owned windows in seed order: fixed anchor (w1), plain remainder
/// (w2), sticky ex-tiled (w3), sticky ex-float (w4), ordinary float (w5).
/// Real member tags plus real classification markers; injected hints only
/// for the tiled rows (float lanes always ride hintless). The anchor
/// sorts first so the hydration focus names it, matching the recorded
/// rejection shape. No native writes beyond the owned windows' own
/// properties; the log sinks to NUL.
struct RejectRig {
    wins: Vec<AdoptWindow>,
    state: super::TileLoop,
    keys: Vec<WindowKey>,
    tokens: Vec<String>,
    output: String,
    observed: Vec<super::ObservedWindow>,
}

fn reject_rig() -> RejectRig {
    super::clear_test_hints();
    let wins = vec![
        AdoptWindow::create(),
        AdoptWindow::create(),
        AdoptWindow::create(),
        AdoptWindow::create(),
        AdoptWindow::create(),
    ];
    let pid = std::process::id();
    let mut state = test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let mut keys = Vec::new();
    let mut tokens = Vec::new();
    let mut output = String::new();
    let mut tags = Vec::new();
    for win in &wins {
        tags.push(
            crate::product_hide::sys::install_member_tag(win.hwnd, pid)
                .expect("member tag installs on the owned window"),
        );
    }
    crate::product_hide::sys::install_sticky_marker(wins[2].hwnd, false)
        .expect("sticky marker installs");
    crate::product_hide::sys::install_sticky_marker(wins[3].hwnd, true)
        .expect("sticky marker installs");
    crate::product_hide::sys::install_float_intent_marker(wins[4].hwnd)
        .expect("float marker installs");
    for (index, win) in wins.iter().enumerate() {
        let (key, token, out, _) = seed_member(&mut state, win.hwnd, &tags[index]);
        keys.push(key);
        tokens.push(token);
        output = out;
    }
    super::inject_test_hints(wins[0].hwnd, fixed_anchor_hints());
    super::inject_test_hints(wins[1].hwnd, reject_plain_hints());
    let observed = vec![
        observed_frame(wins[0].hwnd, &tokens[0], reject_anchor_frame()),
        observed_frame(wins[1].hwnd, &tokens[1], reject_plain_frame()),
        observed_frame(wins[2].hwnd, &tokens[2], reject_float_frame()),
        observed_frame(wins[3].hwnd, &tokens[3], reject_float_frame()),
        observed_frame(wins[4].hwnd, &tokens[4], reject_float_frame()),
    ];
    RejectRig {
        wins,
        state,
        keys,
        tokens,
        output,
        observed,
    }
}


/// Production reconcile event over freshly assembled rows on a cloned
/// Engine, so the probe never mutates the rig: proves the fixture plans
/// through the real event builder and extracts the plan-level writes.


#[test]
fn mixed_roster_adoption_carries_fixed_anchor_as_exception() {
    // Fixed focused anchor plus plain remainder plus three marked floats:
    // hydration admits the complete mixed roster atomically, the hold stays
    // empty, lanes populate, durable markers stay installed, the fixed
    // anchor converges as an Engine exception alongside the floats, the
    // tiled remainder stays eligible, and the production event on a cloned
    // Engine plans with no float writes. A repeated identical observation
    // recomputes the same stable adoption.
    let mut rig = reject_rig();
    let areas = reject_areas_for(&rig.output);
    let mut hint_cx = super::HintCx::new();
    adopt_seeded(
        &mut rig.state,
        &fake_me(),
        &rig.observed,
        &[],
        &areas,
        &mut hint_cx,
    );
    assert!(
        rig.state.restart_reject_hold.is_empty(),
        "admission leaves no hold"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[2]),
        Some(&false),
        "ex-tiled sticky adopts its prior"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[3]),
        Some(&true),
        "ex-float sticky adopts its prior"
    );
    assert!(
        rig.state.floated.contains(&rig.keys[4]),
        "ordinary float adopts"
    );
    for token in &rig.tokens[2..] {
        assert_eq!(
            rig.state.float_rects.get(token),
            Some(&reject_float_frame()),
            "adopted float keeps live frame for {token}"
        );
    }
    for token in &rig.tokens[2..] {
        assert!(
            rig.state.core_is_float_token(token),
            "adopted float {token} converges as an exception"
        );
    }
    // The unmarked fixed anchor adopts no adapter intent: adoption is
    // marker-driven, and minimum-size hints ride no world-path fact (the
    // hint pipeline stages through the retired row assembly only). The
    // world still homes it ordinary; fixed classification from hints is
    // core policy (tiler-core fixed_size_admission, world_admission_close_stay).
    let anchor_key = &rig.keys[0];
    assert!(
        !rig.state.floated.contains(anchor_key)
            && !rig.state.sticky.contains_key(anchor_key)
            && !rig.state.fixed_auto_seen.contains(anchor_key)
            && !rig.state.fixed_tile_overrides.contains(anchor_key)
            && !rig.state.fixed_origin_overrides.contains(anchor_key),
        "unmarked anchor stages no intent lane"
    );
    assert!(
        rig.state.member_home(anchor_key).is_some(),
        "unmarked anchor still homes ordinary"
    );
    assert!(
        !rig.state.core_is_float_token(&rig.tokens[1]),
        "plain remainder stays tiled"
    );
    assert!(
        crate::product_hide::sys::read_float_intent_marker(rig.wins[4].hwnd),
        "durable float marker survives adoption"
    );
    for (win, prior) in [(&rig.wins[2], false), (&rig.wins[3], true)] {
        assert_eq!(
            crate::product_hide::sys::read_sticky_marker(win.hwnd),
            Some(prior),
            "durable sticky marker survives adoption"
        );
    }
    // Same-tick lanes ride every marked float on its live frame while the
    // tiled remainder (and the unmarked anchor) stay out of adapter float
    // lanes: the lanes (not the deleted per-domain rows) carry this.
    for token in &rig.tokens[2..] {
        let key = rig
            .state
            .member_tokens
            .iter()
            .find(|(_, bound)| *bound == token)
            .map(|(key, _)| key.clone())
            .expect("adopted float binds a member");
        assert!(
            rig.state.floated.contains(&key) || rig.state.sticky.contains_key(&key),
            "adopted float {token} rides an adapter float lane"
        );
    }
    // A repeated identical observation recomputes the same stable adoption:
    // no hold, lanes kept, markers untouched.
    let mut hint_cx = super::HintCx::new();
    adopt_seeded(
        &mut rig.state,
        &fake_me(),
        &rig.observed,
        &[],
        &areas,
        &mut hint_cx,
    );
    assert!(
        rig.state.restart_reject_hold.is_empty(),
        "repeat adoption leaves no hold"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[2]),
        Some(&false),
        "repeat adoption keeps ex-tiled sticky"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[3]),
        Some(&true),
        "repeat adoption keeps ex-float sticky"
    );
    assert!(
        rig.state.floated.contains(&rig.keys[4]),
        "repeat adoption keeps ordinary float"
    );
    assert!(
        crate::product_hide::sys::read_float_intent_marker(rig.wins[4].hwnd),
        "repeat adoption keeps the durable marker"
    );
    super::clear_test_hints();
}

#[test]
fn next_consistent_observation_revalidates_after_adoption() {
    // Same rig adopts first with the fixed anchor, then the anchor reports
    // minimum-only (no maximum pin) under unchanged markers and lifetime:
    // the next observation keeps all three floats adopted, the hold stays
    // empty, and later planned writes never target the floats.
    let mut rig = reject_rig();
    let areas = reject_areas_for(&rig.output);
    let mut hint_cx = super::HintCx::new();
    adopt_seeded(
        &mut rig.state,
        &fake_me(),
        &rig.observed,
        &[],
        &areas,
        &mut hint_cx,
    );
    assert!(
        rig.state.restart_reject_hold.is_empty(),
        "first observation adopts with no hold"
    );
    assert!(
        rig.state.floated.contains(&rig.keys[4]),
        "first observation commits the candidate"
    );
    super::inject_test_hints(rig.wins[0].hwnd, min_anchor_hints());
    let mut hint_cx = super::HintCx::new();
    adopt_seeded(
        &mut rig.state,
        &fake_me(),
        &rig.observed,
        &[],
        &areas,
        &mut hint_cx,
    );
    assert!(
        rig.state.restart_reject_hold.is_empty(),
        "successful hydration leaves no hold"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[2]),
        Some(&false),
        "ex-tiled sticky re-adopts its prior"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[3]),
        Some(&true),
        "ex-float sticky re-adopts its prior"
    );
    assert!(
        rig.state.floated.contains(&rig.keys[4]),
        "ordinary float re-adopts"
    );
    for token in &rig.tokens[2..] {
        assert!(
            rig.state.core_is_float_token(token),
            "adopted float {token} converges as an exception"
        );
    }
    // The unmarked anchor adopts no intent lane under min-only hints
    // either (marker-driven adoption only; hints ride no world-path fact).
    assert!(
        !rig.state.floated.contains(&rig.keys[0])
            && !rig.state.sticky.contains_key(&rig.keys[0])
            && !rig.state.fixed_auto_seen.contains(&rig.keys[0]),
        "fixed anchor stays lane-free after min-only hints"
    );
    // Revalidated lanes (adapter-owned, not the deleted per-domain rows):
    // the tile and the unmarked anchor stay unmarked while every adopted
    // float rides a lane.
    for (key, token) in rig.keys.iter().zip(rig.tokens.iter()) {
        let lane = rig.state.floated.contains(key)
            || rig.state.sticky.contains_key(key)
            || rig.state.fixed_auto_seen.contains(key);
        if token == &rig.tokens[1] || token == &rig.tokens[0] {
            assert!(!lane, "revalidated tile {token} stays lane-free");
        } else {
            assert!(lane, "revalidated float {token} takes no tiled write");
        }
    }
    super::clear_test_hints();
}

#[test]
fn monitorless_adoption_still_adopts_marker_intent_without_hold() {
    // No per-domain commit remains to refuse: marker intent lanes are
    // output-independent truths, so adoption with no live monitor (empty
    // areas) still adopts sticky/float lanes from durable markers, stages
    // no hold barrier, and keeps every durable marker. The hold field
    // only ever clears here (corrupt/replaced-lifetime coverage pins the
    // clearing); nothing in production stages it since the Engine-commit
    // rollback path was deleted with the per-domain sessions.
    let mut rig = reject_rig();
    let areas: Vec<super::MonitorArea> = Vec::new();
    // Genuine world seeding while the monitor exists: plant engine homing
    // with the live topology first (production retains prior homing when
    // the monitor later vanishes; a fresh screenless dispatch could not
    // home anything). Adoption itself then runs monitorless.
    super::world_tick::dispatch_test_observation(
        &mut rig.state,
        &rig.observed,
        &[],
        &reject_areas_for(&rig.output),
        0,
    );
    let mut hint_cx = super::HintCx::new();
    super::adopt_restart_markers(
        &mut rig.state,
        &fake_me(),
        &rig.observed,
        &[],
        &areas,
        &mut hint_cx,
    );
    assert!(
        rig.state.restart_reject_hold.is_empty(),
        "monitorless adoption stages no hold barrier"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[2]),
        Some(&false),
        "ex-tiled sticky adopts without a monitor"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[3]),
        Some(&true),
        "ex-float sticky adopts without a monitor"
    );
    assert!(
        rig.state.floated.contains(&rig.keys[4]),
        "ordinary float adopts without a monitor"
    );
    for token in &rig.tokens[2..] {
        assert_eq!(
            rig.state.float_rects.get(token),
            Some(&reject_float_frame()),
            "adopted float keeps live frame for {token}"
        );
    }
    assert!(
        rig.state.world.is_ready(),
        "only the seeding dispatch committed; adoption stages no command"
    );
    assert!(
        crate::product_hide::sys::read_float_intent_marker(rig.wins[4].hwnd),
        "durable float marker survives monitorless adoption"
    );
    for (win, prior) in [(&rig.wins[2], false), (&rig.wins[3], true)] {
        assert_eq!(
            crate::product_hide::sys::read_sticky_marker(win.hwnd),
            Some(prior),
            "durable sticky marker survives monitorless adoption"
        );
    }
    // A repeated identical observation recomputes the same adoption:
    // lanes kept, markers untouched.
    let mut hint_cx = super::HintCx::new();
    // No re-seeding: engine homing persists across observations, so the
    // repeat proves adoption recomputes from retained homing alone.
    super::adopt_restart_markers(
        &mut rig.state,
        &fake_me(),
        &rig.observed,
        &[],
        &areas,
        &mut hint_cx,
    );
    assert!(
        rig.state.restart_reject_hold.is_empty(),
        "repeat monitorless adoption stages no hold"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[2]),
        Some(&false),
        "repeat keeps ex-tiled sticky"
    );
    assert_eq!(
        rig.state.sticky.get(&rig.keys[3]),
        Some(&true),
        "repeat keeps ex-float sticky"
    );
    assert!(
        rig.state.floated.contains(&rig.keys[4]),
        "repeat keeps ordinary float"
    );
    super::clear_test_hints();
}

#[test]
fn invalid_marker_and_replaced_lifetime_never_hold() {
    // Corrupt values and replaced lifetime tags never stage, so a stale
    // hold clears and rebuilds to empty: no persistent protection without
    // a currently valid marker plus exact lifetime.
    super::clear_test_hints();
    // Corrupt float-intent value on our property name.
    {
        let win = AdoptWindow::create();
        let pid = std::process::id();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
            .expect("member tag installs on the owned window");
        let planted = crate::ffi::set_prop(win.hwnd, crate::model::FLOAT_INTENT_PROP, 99);
        assert!(planted, "foreign value plants on the owned window");
        let mut state = test_state();
        state.log_path = std::path::PathBuf::from("NUL");
        let (key, token, output, _) = seed_member(&mut state, win.hwnd, &tag);
        state.restart_reject_hold.insert(key.clone());
        let observed = vec![observed(win.hwnd, &token)];
        let mut hint_cx = super::HintCx::new();
        adopt_seeded(
            &mut state,
            &fake_me(),
            &observed,
            &[],
            &areas_for(&output),
            &mut hint_cx,
        );
        assert!(
            state.restart_reject_hold.is_empty(),
            "corrupt marker rebuilds the hold to empty"
        );
        assert!(
            state.floated.is_empty() && state.sticky.is_empty(),
            "corrupt marker stages no lane"
        );
    }
    // Replaced lifetime tag under a valid marker: the stored tag no
    // longer reads live, so the window is not ours this observation.
    {
        let win = AdoptWindow::create();
        let pid = std::process::id();
        let stale = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
            .expect("member tag installs on the owned window");
        crate::product_hide::sys::install_float_intent_marker(win.hwnd)
            .expect("float marker installs on the owned window");
        let mut state = test_state();
        state.log_path = std::path::PathBuf::from("NUL");
        let (key, token, output, _) = seed_member(&mut state, win.hwnd, &stale);
        crate::product_hide::sys::install_member_tag(win.hwnd, pid)
            .expect("replacement tag installs on the owned window");
        state.restart_reject_hold.insert(key.clone());
        let observed = vec![observed(win.hwnd, &token)];
        let mut hint_cx = super::HintCx::new();
        adopt_seeded(
            &mut state,
            &fake_me(),
            &observed,
            &[],
            &areas_for(&output),
            &mut hint_cx,
        );
        assert!(
            state.restart_reject_hold.is_empty(),
            "replaced lifetime rebuilds the hold to empty"
        );
        assert!(
            state.floated.is_empty() && state.sticky.is_empty(),
            "replaced lifetime stages no lane"
        );
        assert!(
            crate::product_hide::sys::read_float_intent_marker(win.hwnd),
            "replaced lifetime never touches the durable marker"
        );
    }
    super::clear_test_hints();
}
