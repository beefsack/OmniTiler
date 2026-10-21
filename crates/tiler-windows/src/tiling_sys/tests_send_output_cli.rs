#![forbid(unsafe_code)]
use super::poll_send_output_cli_request;
use super::rmax03_adapter_tests::test_state;
use super::workspace_output_send_one;

// Null/disposable-handle harness (mirrors the directional CLI tests):
// every identity below is fake, logs redirect to a temp dir that is
// removed afterwards. Only file and OS-read effects run here; no hooks,
// no setters on foreign windows, no live apps. The three poll tests stop
// before any native read (proof/malformed/owner fences); the routing test
// drives the exact CLI-shaped intent through the production
// `workspace_output_send_one` entry on the hermetic unmanaged path (no
// observation, no Engine mutation).
fn log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-sendout-{name}-{}", std::process::id()));
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
        "follow": true,
        "direction": "right",
        "correlation": "cli-4242-ab12",
    })
    .to_string()
}

fn write_request(dir: &std::path::Path, body: &str) {
    std::fs::write(dir.join(crate::tiling::SEND_OUTPUT_REQUEST_FILE), body).expect("request");
}

fn log_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line"))
        .collect()
}

#[test]
fn send_output_cli_args_parse_follow_stay_and_direction() {
    use crate::tiling::{parse_send_output_args, verify_send_output_argv_consistency};
    let strings = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let follow = parse_send_output_args(&strings(&["--follow", "--direction", "right"]))
        .expect("follow parses");
    assert!(follow.follow);
    assert_eq!(follow.direction, "right");
    verify_send_output_argv_consistency(&strings(&["--follow", "--direction", "right"]), &follow)
        .expect("argv consistent");
    let stay =
        parse_send_output_args(&strings(&["--stay", "--direction", "left"])).expect("stay parses");
    assert!(!stay.follow);
    assert!(parse_send_output_args(&strings(&["--follow"])).is_err());
    assert!(parse_send_output_args(&strings(&["--direction", "right", "--follow"])).is_err());
    assert!(parse_send_output_args(&strings(&["--follow", "--direction", "north"])).is_err());
    assert!(
        parse_send_output_args(&strings(&["--follow", "--direction", "right", "--stay"])).is_err()
    );
    assert!(
        verify_send_output_argv_consistency(&strings(&["--stay", "--direction", "left"]), &follow)
            .is_err()
    );
}

#[test]
fn send_output_request_roundtrip_and_refusals() {
    use crate::tiling::{
        SEND_OUTPUT_REQUEST_VERSION, SendOutputRequest, parse_send_output_request,
        render_send_output_request,
    };
    let request = SendOutputRequest {
        v: SEND_OUTPUT_REQUEST_VERSION,
        creation: "creation".to_owned(),
        pid: 7,
        exe_path: "exe".to_owned(),
        user_sid: "sid".to_owned(),
        session_id: 1,
        follow: false,
        direction: "left".to_owned(),
        correlation: "cli-7-ab12".to_owned(),
    };
    assert_eq!(
        parse_send_output_request(&render_send_output_request(&request)).expect("roundtrip"),
        request
    );
    let mut bad_direction = request.clone();
    bad_direction.direction = "north".to_owned();
    assert!(parse_send_output_request(&render_send_output_request(&bad_direction)).is_err());
    let mut bad_version = request.clone();
    bad_version.v += 1;
    assert!(parse_send_output_request(&render_send_output_request(&bad_version)).is_err());
    let mut bad_identity = request.clone();
    bad_identity.creation.clear();
    assert!(parse_send_output_request(&render_send_output_request(&bad_identity)).is_err());
    assert!(parse_send_output_request("not json").is_err());
}

#[test]
fn send_output_proof_owner_consumes_with_refused_proof() {
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
    poll_send_output_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::SEND_OUTPUT_REQUEST_FILE).exists(),
        "proof refusal still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("send-output"));
    assert_eq!(lines[0]["follow"], serde_json::json!(true));
    assert_eq!(lines[0]["direction"], serde_json::json!("right"));
    assert_eq!(lines[0]["edge"], serde_json::json!("cli"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused-proof"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_output_malformed_body_consumes_with_refused() {
    // A bad body is still consumed exactly once with a refused outcome and
    // no dispatch. Fully hermetic: parsing precedes every native read.
    let dir = log_dir("malformed");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    write_request(&dir, "not json");
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_send_output_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::SEND_OUTPUT_REQUEST_FILE).exists(),
        "malformed body still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("send-output"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_output_owner_mismatch_consumes_with_refused() {
    // Exact owner/session/process binding: a recycled pid (same creation,
    // different pid) refuses with no effect. Fully hermetic: the binding
    // check precedes every native read.
    let dir = log_dir("mismatch");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    write_request(&dir, &owner_body(&me, me.pid + 1));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_send_output_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::SEND_OUTPUT_REQUEST_FILE).exists(),
        "mismatched owner still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("send-output"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_output_cli_intent_routes_through_production_dispatch() {
    // The exact intent shape the poll builds (direction/follow from the
    // validated request, down edge, consumed/announced, live-foreground
    // origin slot) routes through the production `workspace_output_send_one`
    // entry. With no member behind the origin token it settles `unmanaged`
    // with no Engine, focus, or view mutation. Fully hermetic: the
    // member-lookup path runs before any observation or native read.
    let dir = log_dir("route");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let origin = crate::snapkey::SnapOrigin {
        hwnd: 0,
        pid: 0,
        creation: String::new(),
        token: "missing-token".to_owned(),
    };
    let intent = crate::snapkey::QueuedWorkspaceOutputIntent {
        direction: tiler_core::directional::Direction::Right,
        follow: true,
        edge: crate::snapkey::SnapEdge::Down,
        origin: Some(origin.clone()),
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let mut observed = Vec::new();
    let retained = Vec::new();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    let outcome = workspace_output_send_one(
        &mut state,
        &me,
        &store,
        &dir,
        &[],
        &[],
        &mut observed,
        &retained,
        "output-1",
        &intent,
        &origin,
        1,
    );
    assert_eq!(outcome, "unmanaged");
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("workspace-output"));
    assert_eq!(lines[0]["direction"], serde_json::json!("right"));
    assert_eq!(lines[0]["follow"], serde_json::json!(true));
    assert_eq!(lines[0]["outcome"], serde_json::json!("unmanaged"));
    let _ = std::fs::remove_dir_all(&dir);
}
