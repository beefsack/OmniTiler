#![forbid(unsafe_code)]
//! F29 control-ingress regressions: every Windows command-file receiver reads
//! within one byte bound and consumes an oversized or non-UTF-8 body exactly
//! once with a bounded refusal, while every request codec rejects unknown
//! fields and still accepts the exact bodies its renderers emit. Fully
//! hermetic: fake identities, temp dirs only, no native reads or writes.

use super::rmax03_adapter_tests::test_state;
use super::{
    WORKSPACE_REQUEST_FILE, poll_directional_cli_request, poll_migrate_workspace_cli_request,
    poll_resize_cli_request, poll_send_output_cli_request, poll_workspace_cli_request,
};
use crate::tiling::{
    CONTROL_REQUEST_MAX_BYTES, ControlRequestRead, DIRECTIONAL_REQUEST_FILE,
    DIRECTIONAL_REQUEST_VERSION, DirectionalOp, DirectionalRequest, MIGRATE_WORKSPACE_REQUEST_FILE,
    MIGRATE_WORKSPACE_REQUEST_VERSION, MigrateWorkspaceRequest, RESIZE_REQUEST_FILE,
    RESIZE_REQUEST_VERSION, ResizeRequest, SEND_OUTPUT_REQUEST_FILE, SEND_OUTPUT_REQUEST_VERSION,
    SendOutputRequest, WORKSPACE_REQUEST_VERSION, WorkspaceAction, WorkspaceRequest,
    parse_directional_request, parse_migrate_workspace_request, parse_resize_request,
    parse_send_output_request, parse_workspace_request, read_control_request,
    render_directional_request, render_migrate_workspace_request, render_resize_request,
    render_send_output_request, render_workspace_request,
};

fn control_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-control-{name}-{}", std::process::id()));
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

fn log_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line"))
        .collect()
}

fn with_unknown_field(body: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(body).expect("rendered json");
    value["unexpected"] = serde_json::json!(true);
    value.to_string()
}

/// One representative valid request per ingress verb. Each renderer output
/// must keep parsing (valid clients unchanged) while the same body with one
/// extra field is refused by the strict codec.
fn valid_request_bodies() -> Vec<(&'static str, String)> {
    let workspace = WorkspaceRequest {
        v: WORKSPACE_REQUEST_VERSION,
        creation: "test-creation".to_owned(),
        pid: 4242,
        exe_path: "test.exe".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        action: WorkspaceAction::Select,
        index: 0,
        direction: None,
        correlation: "cli-4242-ab12".to_owned(),
    };
    let resize = ResizeRequest {
        v: RESIZE_REQUEST_VERSION,
        creation: "test-creation".to_owned(),
        pid: 4242,
        exe_path: "test.exe".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        direction: "left".to_owned(),
        mode: "outwards".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let directional = DirectionalRequest {
        v: DIRECTIONAL_REQUEST_VERSION,
        creation: "test-creation".to_owned(),
        pid: 4242,
        exe_path: "test.exe".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        op: DirectionalOp::Move,
        direction: "right".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let send_output = SendOutputRequest {
        v: SEND_OUTPUT_REQUEST_VERSION,
        creation: "test-creation".to_owned(),
        pid: 4242,
        exe_path: "test.exe".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        follow: false,
        direction: "left".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let migrate = MigrateWorkspaceRequest {
        v: MIGRATE_WORKSPACE_REQUEST_VERSION,
        creation: "test-creation".to_owned(),
        pid: 4242,
        exe_path: "test.exe".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        direction: "up".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    vec![
        ("workspace", render_workspace_request(&workspace)),
        ("resize", render_resize_request(&resize)),
        ("directional", render_directional_request(&directional)),
        ("send-output", render_send_output_request(&send_output)),
        (
            "migrate-workspace",
            render_migrate_workspace_request(&migrate),
        ),
    ]
}

#[test]
fn control_request_reader_accepts_within_cap_and_rejects_over_cap() {
    let dir = control_dir("reader");
    let path = dir.join("request");

    assert_eq!(read_control_request(&path), ControlRequestRead::Missing);

    std::fs::write(&path, b"{}").expect("small body");
    assert_eq!(
        read_control_request(&path),
        ControlRequestRead::Body("{}".to_owned())
    );

    let exact = vec![b'x'; CONTROL_REQUEST_MAX_BYTES as usize];
    std::fs::write(&path, &exact).expect("cap body");
    match read_control_request(&path) {
        ControlRequestRead::Body(body) => assert_eq!(body.len(), exact.len()),
        other => panic!("exact-cap body must read, got {other:?}"),
    }

    let over = vec![b'x'; CONTROL_REQUEST_MAX_BYTES as usize + 1];
    std::fs::write(&path, &over).expect("over-cap body");
    assert_eq!(read_control_request(&path), ControlRequestRead::Rejected);

    std::fs::write(&path, [0xff, 0xfe, 0xfd]).expect("invalid utf8");
    assert_eq!(read_control_request(&path), ControlRequestRead::Rejected);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn control_request_codecs_reject_unknown_fields() {
    for (name, body) in valid_request_bodies() {
        let valid = match name {
            "workspace" => parse_workspace_request(&body).is_ok(),
            "resize" => parse_resize_request(&body).is_ok(),
            "directional" => parse_directional_request(&body).is_ok(),
            "send-output" => parse_send_output_request(&body).is_ok(),
            "migrate-workspace" => parse_migrate_workspace_request(&body).is_ok(),
            _ => unreachable!("known request codec"),
        };
        assert!(valid, "{name} canonical body still parses");
        let with_extra = with_unknown_field(&body);
        let refused = match name {
            "workspace" => parse_workspace_request(&with_extra).is_err(),
            "resize" => parse_resize_request(&with_extra).is_err(),
            "directional" => parse_directional_request(&with_extra).is_err(),
            "send-output" => parse_send_output_request(&with_extra).is_err(),
            "migrate-workspace" => parse_migrate_workspace_request(&with_extra).is_err(),
            _ => unreachable!("known request codec"),
        };
        assert!(refused, "{name} must reject unknown fields");
    }
}

#[test]
fn control_unknown_field_ingress_consumes_with_refused() {
    // A body with every required field plus one unrecognized field is
    // consumed exactly once and refused; no dispatch path runs.
    let dir = control_dir("unknown-field");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    let body = valid_request_bodies()
        .into_iter()
        .find(|(name, _)| *name == "workspace")
        .map(|(_, body)| body)
        .expect("workspace body");
    std::fs::write(dir.join(WORKSPACE_REQUEST_FILE), with_unknown_field(&body))
        .expect("workspace body");

    poll_workspace_cli_request(&mut state, &me, &store, &dir, &[], &[]);

    assert!(
        !dir.join(WORKSPACE_REQUEST_FILE).exists(),
        "unknown-field body still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("workspace"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn control_oversized_bodies_are_consumed_once_with_bounded_refusal() {
    let dir = control_dir("oversized");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    let secret = format!(
        "{{\"secret\":\"{}\"}}",
        "do-not-log".repeat(CONTROL_REQUEST_MAX_BYTES as usize / 10 + 1)
    );
    assert!(secret.len() as u64 > CONTROL_REQUEST_MAX_BYTES);

    std::fs::write(dir.join(WORKSPACE_REQUEST_FILE), &secret).expect("workspace body");
    poll_workspace_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(WORKSPACE_REQUEST_FILE).exists(),
        "workspace consumed"
    );

    std::fs::write(dir.join(RESIZE_REQUEST_FILE), &secret).expect("resize body");
    poll_resize_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(!dir.join(RESIZE_REQUEST_FILE).exists(), "resize consumed");

    std::fs::write(dir.join(DIRECTIONAL_REQUEST_FILE), &secret).expect("directional body");
    poll_directional_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(DIRECTIONAL_REQUEST_FILE).exists(),
        "directional consumed"
    );

    std::fs::write(dir.join(SEND_OUTPUT_REQUEST_FILE), &secret).expect("send-output body");
    poll_send_output_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(SEND_OUTPUT_REQUEST_FILE).exists(),
        "send-output consumed"
    );

    std::fs::write(dir.join(MIGRATE_WORKSPACE_REQUEST_FILE), &secret).expect("migrate body");
    poll_migrate_workspace_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(MIGRATE_WORKSPACE_REQUEST_FILE).exists(),
        "migrate-workspace consumed"
    );

    assert_eq!(state.tick, 5, "one bounded refusal per verb");
    let lines = log_lines(&dir);
    let events: Vec<&str> = lines
        .iter()
        .map(|line| line["event"].as_str().expect("event string"))
        .collect();
    assert_eq!(
        events,
        [
            "workspace",
            "resize",
            "directional",
            "send-output",
            "migrate-workspace"
        ]
    );
    assert!(lines.iter().all(|line| line["outcome"] == "refused"));
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    assert!(
        !text.contains("do-not-log") && !text.contains("secret"),
        "oversized body bytes never reach the log"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
