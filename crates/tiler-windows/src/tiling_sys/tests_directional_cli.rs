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
