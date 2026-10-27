#![forbid(unsafe_code)]
use super::rmax03_adapter_tests::test_state;
use super::{keyboard_tick, poll_directional_cli_request};

/// Null/disposable-handle harness (mirrors the orientation route tests):
/// every identity below is fake, logs redirect to a temp dir that is
/// removed afterwards. Only file and OS-read effects run here; no hooks,
/// no setters on foreign windows, no live apps. The three poll tests stop
/// before any native read (proof/malformed/owner fences); the routing test
/// drives the exact CLI-shaped intent through the production `keyboard_tick`
/// directional arm on the hermetic origin-vanished path (no observation).
fn log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-dir-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

fn fake_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

/// Revealed-window fixture for the post-reveal stale-cache shape (a
/// foreground return proves focus on the returned window while the cached
/// origins still name the pre-reveal set): one eligible observed window
/// with plainly synthetic identity. No host values are cited or copied;
/// every native query against the fake HWND fails closed and nothing
/// actuates.
fn revealed_window(me: &crate::model::ProcessIdentity) -> super::ObservedWindow {
    use tiler_core::geometry::Rect;
    let rect = Rect {
        x: 10,
        y: 10,
        w: 800,
        h: 560,
    };
    super::ObservedWindow {
        hwnd: 0x4311,
        token: "tok-reveal".to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::measure(rect, rect),
        identity: super::ObservedTarget {
            hwnd: 0x4311,
            pid: 4243,
            process_creation: "c-4311".to_owned(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: "tag-4311".to_owned(),
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
        member_tag: Some("tag-4311".to_owned()),
    }
}

fn focus_body(me: &crate::model::ProcessIdentity) -> String {
    serde_json::json!({
        "v": 1,
        "creation": me.process_creation,
        "pid": me.pid,
        "exe_path": me.exe_path,
        "user_sid": me.user_sid,
        "session_id": me.session_id,
        "op": "focus",
        "direction": "right",
        "correlation": "cli-4242-cd34",
    })
    .to_string()
}

fn directional_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
    log_lines(dir)
        .into_iter()
        .filter(|line| line["event"] == serde_json::json!("directional"))
        .collect()
}

fn owner_body(me: &crate::model::ProcessIdentity, pid: u32) -> String {
    serde_json::json!({
        "v": 1,
        "creation": me.process_creation,
        "pid": pid,
        "exe_path": me.exe_path,
        "user_sid": me.user_sid,
        "session_id": me.session_id,
        "op": "move",
        "direction": "left",
        "correlation": "cli-4242-ab12",
    })
    .to_string()
}

fn write_request(dir: &std::path::Path, body: &str) {
    std::fs::write(dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE), body).expect("request");
}

fn log_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line"))
        .collect()
}

#[test]
fn directional_proof_owner_consumes_with_refused_proof() {
    // Proof owners never serve the normal control: the request is consumed
    // once with an honest refusal and no native effect, so the queue cannot
    // wedge. Fully hermetic: no native read runs before the proof fence.
    let dir = log_dir("proof");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    state.allowlist = Some(Vec::new());
    let me = fake_me();
    write_request(&dir, &owner_body(&me, me.pid));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "proof refusal still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("directional"));
    assert_eq!(lines[0]["op"], serde_json::json!("move"));
    assert_eq!(lines[0]["direction"], serde_json::json!("left"));
    assert_eq!(lines[0]["edge"], serde_json::json!("cli"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused-proof"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_malformed_body_consumes_with_refused() {
    // A bad body is still consumed exactly once with a refused outcome and
    // no dispatch. Fully hermetic: parsing precedes every native read.
    let dir = log_dir("malformed");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    write_request(&dir, "not json");
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "malformed body still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("directional"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_owner_mismatch_consumes_with_refused() {
    // Exact owner/session/process binding: a recycled pid (same creation,
    // different pid) refuses with no effect. Fully hermetic: the binding
    // check precedes every native read.
    let dir = log_dir("mismatch");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    write_request(&dir, &owner_body(&me, me.pid + 1));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "mismatched owner still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("directional"));
    assert_eq!(lines[0]["op"], serde_json::json!("move"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_cli_intent_routes_through_production_dispatch() {
    // The exact intent shape the poll builds (op/direction from the
    // validated request, down edge, consumed/announced, live-foreground
    // origin slot) routes through the production `keyboard_tick`
    // directional arm. With no origin it settles `origin-vanished` through
    // `dispatch_directional_intent` with no Engine, focus, or view mutation
    // and exactly one tick. Fully hermetic: the origin-vanished path runs
    // before any observation or native read.
    let dir = log_dir("route");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    keyboard_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &[],
        &[],
        vec![crate::snapkey::QueuedSnapEvent::Intent(
            crate::snapkey::QueuedIntent {
                op: crate::snapkey::SnapOp::Move,
                direction: tiler_core::directional::Direction::Left,
                edge: crate::snapkey::SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick: std::time::Instant::now(),
            },
        )],
        None,
    );
    assert_eq!(state.tick, 1, "production dispatch advances one tick");
    assert!(state.snap_advance.is_none());
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("snap"));
    assert_eq!(lines[0]["op"], serde_json::json!("move"));
    assert_eq!(lines[0]["direction"], serde_json::json!("left"));
    assert_eq!(lines[0]["edge"], serde_json::json!("down"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("origin-vanished"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_cli_refreshes_stale_origins_after_reveal() {
    // Post-reveal stale-cache shape (proven focus on the returned window
    // while the cached `snap_origins` still names the pre-reveal set,
    // empty here): the next directional CLI must accept the correctly
    // bound intent from a fresh observation instead of settling
    // `unmanaged` on the stale cache. Production path: fresh `observe` +
    // `publish_managed` inside the poll, then the real `keyboard_tick`
    // directional dispatch (which re-observes and safely refuses with no
    // world accept or actuation proof). Known
    // limitation kept explicit: this harness seeds no member tables, so
    // the downstream dispatch always refuses; the regression pins the CLI
    // gate verdict plus the no-effects boundary, not a full applied move.
    // Gate reads are injected; downstream foreground remains a read-only
    // ambient query. Fake HWNDs and unseeded membership prevent actuation.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend, inject_pending_tag,
    };
    clear_pending_seams();
    let dir = log_dir("stale-reveal");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    assert!(
        state.snap_origins.is_empty(),
        "stale post-reveal cache starts empty"
    );
    let me = fake_me();
    let window = revealed_window(&me);
    inject_pending_observe(vec![window], Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0x4311));
    inject_pending_tag(0x4311, Some("tag-4311".to_owned()));
    write_request(&dir, &focus_body(&me));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "request consumed exactly once"
    );
    let cli = directional_lines(&dir);
    assert_eq!(cli.len(), 1, "one cli verdict line: {cli:?}");
    assert_eq!(cli[0]["op"], serde_json::json!("focus"));
    assert_eq!(cli[0]["direction"], serde_json::json!("right"));
    assert_eq!(cli[0]["edge"], serde_json::json!("cli"));
    assert_eq!(
        cli[0]["outcome"],
        serde_json::json!("dispatched"),
        "fresh origins accept the revealed foreground"
    );
    assert_eq!(cli[0]["origin"], serde_json::json!("tok-reveal"));
    assert_eq!(cli[0]["tick"], serde_json::json!(1));
    assert!(
        state
            .snap_origins
            .get(&0x4311)
            .is_some_and(|o| o.token == "tok-reveal"),
        "gate read refreshed origins, not the stale cache"
    );
    assert_eq!(
        state.tick, 2,
        "cli verdict plus one production dispatch turn"
    );
    assert!(state.snap_advance.is_none(), "no advance on safe refusal");
    assert!(state.member_tokens.is_empty(), "no admission side effects");
    assert!(
        state.world.last_reply().is_none(),
        "downstream refusal accepts nothing"
    );
    let snaps: Vec<serde_json::Value> = log_lines(&dir)
        .into_iter()
        .filter(|line| line["event"] == serde_json::json!("snap"))
        .collect();
    assert_eq!(snaps.len(), 1, "one downstream dispatch line: {snaps:?}");
    assert!(
        snaps[0]["outcome"] != serde_json::json!("focus-ok")
            && snaps[0]["outcome"] != serde_json::json!("move-applied"),
        "no proven actuation downstream: {}",
        snaps[0]["outcome"]
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_cli_foreign_foreground_stays_unmanaged_without_effects() {
    // Discriminating safety net for the same gate: a foreground HWND the
    // fresh observation never names (foreign/unmanaged) still settles
    // `unmanaged` with no dispatch and no state effects. The refreshed
    // origins (fixture window present) prove the verdict came from fresh
    // data, not the stale cache.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend, inject_pending_tag,
    };
    clear_pending_seams();
    let dir = log_dir("foreign-fg");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let window = revealed_window(&me);
    inject_pending_observe(vec![window], Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0x4999));
    inject_pending_tag(0x4311, Some("tag-4311".to_owned()));
    write_request(&dir, &focus_body(&me));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "request consumed exactly once"
    );
    let cli = directional_lines(&dir);
    assert_eq!(cli.len(), 1, "one cli verdict line: {cli:?}");
    assert_eq!(
        cli[0]["outcome"],
        serde_json::json!("unmanaged"),
        "foreign foreground never dispatches"
    );
    assert_eq!(state.tick, 1, "no dispatch turn runs");
    assert!(
        state
            .snap_origins
            .get(&0x4311)
            .is_some_and(|o| o.token == "tok-reveal"),
        "verdict used the fresh observation, not the stale cache"
    );
    assert!(state.snap_advance.is_none());
    assert!(state.member_tokens.is_empty(), "no admission side effects");
    assert!(state.world.last_reply().is_none(), "nothing accepted");
    let snaps: Vec<serde_json::Value> = log_lines(&dir)
        .into_iter()
        .filter(|line| line["event"] == serde_json::json!("snap"))
        .collect();
    assert!(snaps.is_empty(), "no dispatch, no snap lines");
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plant one stale cached origin: the pre-reveal map entry a refresh must
/// replace, never reuse. Plainly synthetic, never a host value.
fn stale_origin(hwnd: u64) -> crate::snapkey::SnapOrigin {
    crate::snapkey::SnapOrigin {
        hwnd,
        token: "tok-stale".to_owned(),
        pid: 4243,
        creation: "c-stale".to_owned(),
    }
}

fn stale_singleton(state: &super::TileLoop) -> bool {
    state.snap_origins.len() == 1
        && state
            .snap_origins
            .get(&0x4311)
            .is_some_and(|o| o.token == "tok-stale")
}

#[test]
fn directional_cli_observation_failure_never_uses_stale_origins() {
    // A failed enumeration must not fall back to the stale cached origin:
    // the poll settles `unmanaged` with the stale entry untouched, no
    // dispatch, and no state effects.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend,
    };
    clear_pending_seams();
    let dir = log_dir("observe-failed");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    state.snap_origins.insert(0x4311, stale_origin(0x4311));
    let me = fake_me();
    inject_pending_observe(Vec::new(), Vec::new(), true);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0x4311));
    write_request(&dir, &focus_body(&me));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "request consumed exactly once"
    );
    let cli = directional_lines(&dir);
    assert_eq!(cli.len(), 1, "one cli verdict line: {cli:?}");
    assert_eq!(
        cli[0]["outcome"],
        serde_json::json!("unmanaged"),
        "failed observation never dispatches"
    );
    assert_eq!(state.tick, 1, "no dispatch turn runs");
    assert!(
        stale_singleton(&state),
        "stale entry untouched and unusable: {:?}",
        state.snap_origins
    );
    assert!(state.member_tokens.is_empty(), "no admission side effects");
    assert!(state.world.last_reply().is_none(), "nothing accepted");
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_cli_suspend_fence_runs_before_observation() {
    // The suspension fence stays ahead of the origin refresh: a blocked
    // session settles `suspended` without touching the stale cache, even
    // with an eligible fixture that would otherwise dispatch.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_observe,
        inject_pending_suspend, inject_pending_tag,
    };
    clear_pending_seams();
    let dir = log_dir("suspend-fence");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    state.snap_origins.insert(0x4311, stale_origin(0x4311));
    let me = fake_me();
    inject_pending_observe(vec![revealed_window(&me)], Vec::new(), false);
    inject_pending_suspend(Some(true));
    inject_pending_elevated(Some(false));
    inject_pending_tag(0x4311, Some("tag-4311".to_owned()));
    write_request(&dir, &focus_body(&me));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "request consumed exactly once"
    );
    let cli = directional_lines(&dir);
    assert_eq!(cli.len(), 1, "one cli verdict line: {cli:?}");
    assert_eq!(
        cli[0]["outcome"],
        serde_json::json!("suspended"),
        "suspension settles before any observation"
    );
    assert_eq!(state.tick, 1, "no dispatch turn runs");
    assert!(
        stale_singleton(&state),
        "no refresh ran under suspension: {:?}",
        state.snap_origins
    );
    assert!(state.member_tokens.is_empty(), "no admission side effects");
    assert!(state.world.last_reply().is_none(), "nothing accepted");
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_cli_elevated_fence_runs_before_observation() {
    // Same ordering for the elevated-foreground fence: it settles
    // `elevated-foreground` without touching the stale cache, even with an
    // eligible fixture that would otherwise dispatch.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_observe,
        inject_pending_suspend, inject_pending_tag,
    };
    clear_pending_seams();
    let dir = log_dir("elevated-fence");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    state.snap_origins.insert(0x4311, stale_origin(0x4311));
    let me = fake_me();
    inject_pending_observe(vec![revealed_window(&me)], Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(true));
    inject_pending_tag(0x4311, Some("tag-4311".to_owned()));
    write_request(&dir, &focus_body(&me));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "request consumed exactly once"
    );
    let cli = directional_lines(&dir);
    assert_eq!(cli.len(), 1, "one cli verdict line: {cli:?}");
    assert_eq!(
        cli[0]["outcome"],
        serde_json::json!("elevated-foreground"),
        "elevation settles before any observation"
    );
    assert_eq!(state.tick, 1, "no dispatch turn runs");
    assert!(
        stale_singleton(&state),
        "no refresh ran under elevation: {:?}",
        state.snap_origins
    );
    assert!(state.member_tokens.is_empty(), "no admission side effects");
    assert!(state.world.last_reply().is_none(), "nothing accepted");
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directional_cli_proof_fence_runs_before_observation() {
    // Proof owners refuse before any observation on the new path too: an
    // injected eligible fixture must not refresh the cache or dispatch.
    use super::{clear_pending_seams, inject_pending_observe};
    clear_pending_seams();
    let dir = log_dir("proof-fence");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    state.allowlist = Some(Vec::new());
    state.snap_origins.insert(0x4311, stale_origin(0x4311));
    let me = fake_me();
    inject_pending_observe(vec![revealed_window(&me)], Vec::new(), false);
    write_request(&dir, &focus_body(&me));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE).exists(),
        "proof refusal still consumes the request"
    );
    let cli = directional_lines(&dir);
    assert_eq!(cli.len(), 1, "one cli verdict line: {cli:?}");
    assert_eq!(
        cli[0]["outcome"],
        serde_json::json!("refused-proof"),
        "proof owners never reach observation"
    );
    assert_eq!(state.tick, 1, "no dispatch turn runs");
    assert!(
        stale_singleton(&state),
        "no refresh ran for proof owners: {:?}",
        state.snap_origins
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}
