#![forbid(unsafe_code)]
use super::poll_migrate_workspace_cli_request;
use super::rmax03_adapter_tests::test_state;
use super::workspace_migrate_one;

// Null/disposable-handle harness (mirrors the send-output CLI tests):
// every identity below is fake, logs redirect to a temp dir that is
// removed afterwards. Only file and OS-read effects run here; no hooks,
// no setters on foreign windows, no live apps. The three poll tests stop
// before any native read (proof/malformed/owner fences); the routing test
// drives the exact CLI-shaped intent through the production
// `workspace_migrate_one` entry on the hermetic unknown-output path (no
// observation, no Engine mutation).
fn log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-migws-{name}-{}", std::process::id()));
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
        "direction": "right",
        "correlation": "cli-4242-ab12",
    })
    .to_string()
}

fn write_request(dir: &std::path::Path, body: &str) {
    std::fs::write(
        dir.join(crate::tiling::MIGRATE_WORKSPACE_REQUEST_FILE),
        body,
    )
    .expect("request");
}

fn log_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line"))
        .collect()
}

#[test]
fn migrate_workspace_cli_args_parse_direction() {
    use crate::tiling::{parse_migrate_workspace_args, verify_migrate_workspace_argv_consistency};
    let strings = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let right = parse_migrate_workspace_args(&strings(&["--direction", "right"]))
        .expect("direction parses");
    assert_eq!(right.direction, "right");
    verify_migrate_workspace_argv_consistency(&strings(&["--direction", "right"]), &right)
        .expect("argv consistent");
    assert!(parse_migrate_workspace_args(&strings(&[])).is_err());
    assert!(parse_migrate_workspace_args(&strings(&["--direction"])).is_err());
    assert!(parse_migrate_workspace_args(&strings(&["--direction", "right", "--trace"])).is_err());
    assert!(parse_migrate_workspace_args(&strings(&["right", "--direction"])).is_err());
    assert!(parse_migrate_workspace_args(&strings(&["--direction", "north"])).is_err());
    assert!(
        verify_migrate_workspace_argv_consistency(&strings(&["--direction", "left"]), &right)
            .is_err()
    );
}

#[test]
fn migrate_workspace_request_roundtrip_and_refusals() {
    use crate::tiling::{
        MIGRATE_WORKSPACE_REQUEST_VERSION, MigrateWorkspaceRequest,
        parse_migrate_workspace_request, render_migrate_workspace_request,
    };
    let request = MigrateWorkspaceRequest {
        v: MIGRATE_WORKSPACE_REQUEST_VERSION,
        creation: "creation".to_owned(),
        pid: 7,
        exe_path: "exe".to_owned(),
        user_sid: "sid".to_owned(),
        session_id: 1,
        direction: "left".to_owned(),
        correlation: "cli-7-ab12".to_owned(),
    };
    assert_eq!(
        parse_migrate_workspace_request(&render_migrate_workspace_request(&request))
            .expect("roundtrip"),
        request
    );
    let mut bad_direction = request.clone();
    bad_direction.direction = "north".to_owned();
    assert!(
        parse_migrate_workspace_request(&render_migrate_workspace_request(&bad_direction)).is_err()
    );
    let mut bad_version = request.clone();
    bad_version.v += 1;
    assert!(
        parse_migrate_workspace_request(&render_migrate_workspace_request(&bad_version)).is_err()
    );
    let mut bad_identity = request.clone();
    bad_identity.creation.clear();
    assert!(
        parse_migrate_workspace_request(&render_migrate_workspace_request(&bad_identity)).is_err()
    );
    assert!(parse_migrate_workspace_request("not json").is_err());
}

#[test]
fn migrate_workspace_proof_owner_consumes_with_refused_proof() {
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
    poll_migrate_workspace_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::MIGRATE_WORKSPACE_REQUEST_FILE)
            .exists(),
        "proof refusal still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("migrate-workspace"));
    assert_eq!(lines[0]["direction"], serde_json::json!("right"));
    assert_eq!(lines[0]["edge"], serde_json::json!("cli"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused-proof"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrate_workspace_malformed_body_consumes_with_refused() {
    // A bad body is still consumed exactly once with a refused outcome and
    // no dispatch. Fully hermetic: parsing precedes every native read.
    let dir = log_dir("malformed");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    write_request(&dir, "not json");
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_migrate_workspace_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::MIGRATE_WORKSPACE_REQUEST_FILE)
            .exists(),
        "malformed body still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("migrate-workspace"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrate_workspace_owner_mismatch_consumes_with_refused() {
    // Exact owner/session/process binding: a recycled pid (same creation,
    // different pid) refuses with no effect. Fully hermetic: the binding
    // check precedes every native read.
    let dir = log_dir("mismatch");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    write_request(&dir, &owner_body(&me, me.pid + 1));
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    poll_migrate_workspace_cli_request(&mut state, &me, &store, &dir, &[], &[]);
    assert!(
        !dir.join(crate::tiling::MIGRATE_WORKSPACE_REQUEST_FILE)
            .exists(),
        "mismatched owner still consumes the request"
    );
    assert_eq!(state.tick, 1);
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("migrate-workspace"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("refused"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrate_workspace_cli_intent_routes_through_production_dispatch() {
    // The exact intent shape the poll builds (direction from the validated
    // request, down edge, consumed/announced, no foreground origin) routes
    // through the production `workspace_migrate_one` entry. With no output
    // token behind the device name it settles `unknown-output` with no
    // Engine, focus, or view mutation. Fully hermetic: the token-lookup path
    // runs before any observation or native read.
    let dir = log_dir("route");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let intent = crate::snapkey::QueuedWorkspaceMigrateIntent {
        direction: tiler_core::directional::Direction::Right,
        edge: crate::snapkey::SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    let outcome = workspace_migrate_one(
        &mut state,
        &me,
        &store,
        &dir,
        &[],
        &[],
        &mut [],
        &[],
        "output-1",
        &intent,
        1,
    );
    assert_eq!(outcome, "unknown-output");
    let lines = log_lines(&dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], serde_json::json!("workspace-migrate"));
    assert_eq!(lines[0]["direction"], serde_json::json!("right"));
    assert_eq!(lines[0]["outcome"], serde_json::json!("unknown-output"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrate_workspace_applied_path_through_production_route() {
    // Real applied command path with a scripted native seam (no live
    // effects): two outputs, five managed windows, one observation tick
    // to plant genuine core homing plus both views, then the exact
    // CLI-shaped intent through the production `workspace_migrate_one`
    // entry. The command must apply (core accepts, adapter reports
    // `migrate-applied`) with a retained migration transfer. Fully
    // hermetic: injected observation/suspend/elevated/foreground plus
    // fake HWNDs (actuation fails closed, no hooks or setters on live
    // windows). This is the positive coverage the live leg lacked:
    // `migrate-applied` had never been proven anywhere before.
    use super::{
        MonitorArea, ObservedTarget, ObservedWindow, clear_pending_seams, inject_pending_elevated,
        inject_pending_foreground, inject_pending_observe, inject_pending_suspend,
        inject_pending_tag,
    };
    use tiler_core::geometry::Rect;
    clear_pending_seams();
    let dir = log_dir("applied");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let areas = vec![
        MonitorArea {
            device: "DEV1".to_owned(),
            work: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        },
        MonitorArea {
            device: "DEV2".to_owned(),
            work: Rect {
                x: 800,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x: 800,
                y: 0,
                w: 800,
                h: 600,
            },
        },
    ];
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.ensure_device("DEV1");
    state.ensure_device("DEV2");
    let wins: Vec<(u64, &str, u32, &str, Rect)> = vec![
        (
            0x4101,
            "w-m1",
            7101,
            "c-4101",
            Rect {
                x: 10,
                y: 10,
                w: 200,
                h: 200,
            },
        ),
        (
            0x4102,
            "w-m2",
            7102,
            "c-4102",
            Rect {
                x: 300,
                y: 10,
                w: 200,
                h: 200,
            },
        ),
        (
            0x4103,
            "w-m3",
            7103,
            "c-4103",
            Rect {
                x: 300,
                y: 300,
                w: 200,
                h: 200,
            },
        ),
        (
            0x4104,
            "w-n1",
            7104,
            "c-4104",
            Rect {
                x: 810,
                y: 10,
                w: 200,
                h: 200,
            },
        ),
        (
            0x4105,
            "w-n2",
            7105,
            "c-4105",
            Rect {
                x: 1100,
                y: 10,
                w: 200,
                h: 200,
            },
        ),
    ];
    let plain = || crate::tiling::WindowFacts {
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
    };
    for (hwnd, token, pid, creation, _rect) in &wins {
        let key = crate::native_guards::WindowKey {
            hwnd: *hwnd,
            pid: *pid,
            creation: (*creation).to_owned(),
        };
        state.member_tokens.insert(key.clone(), (*token).to_owned());
        state.member_identity.insert(
            key.clone(),
            crate::model::ProcessIdentity {
                pid: *pid,
                process_creation: (*creation).to_owned(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state.member_tags.insert(key.clone(), format!("tag-{hwnd}"));
        inject_pending_tag(*hwnd, Some(format!("tag-{hwnd}")));
    }
    let observed: Vec<ObservedWindow> = wins
        .iter()
        .map(|(hwnd, token, pid, creation, rect)| ObservedWindow {
            hwnd: *hwnd,
            token: (*token).to_owned(),
            outer: *rect,
            visible: *rect,
            insets: crate::tiling::FrameInsets::measure(*rect, *rect),
            identity: ObservedTarget {
                hwnd: *hwnd,
                pid: *pid,
                process_creation: (*creation).to_owned(),
                exe_path: me.exe_path.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                tag: format!("tag-{hwnd}"),
            },
            facts: plain(),
            member_tag: Some(format!("tag-{hwnd}")),
        })
        .collect();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    let seed = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
    )
    .expect("seed tick converges");
    assert!(seed.ready, "seed converges");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    for (hwnd, _t, _p, _c, _r) in &wins {
        inject_pending_tag(*hwnd, Some(format!("tag-{hwnd}")));
    }
    let intent = crate::snapkey::QueuedWorkspaceMigrateIntent {
        direction: tiler_core::directional::Direction::Right,
        edge: crate::snapkey::SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let outcome = workspace_migrate_one(
        &mut state,
        &me,
        &store,
        &dir,
        &fulls,
        &areas,
        &mut [],
        &[],
        "DEV1",
        &intent,
        99,
    );
    assert_eq!(outcome, "migrate-applied", "healthy roster applies");
    let lines = log_lines(&dir);
    assert!(
        lines.iter().any(
            |line| line["event"] == serde_json::json!("workspace-migrate")
                && line["outcome"] == serde_json::json!("migrate-applied")
        ),
        "applied outcome logged: {lines:?}"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrate_workspace_refusal_logs_bounded_core_reason() {
    use super::{
        MonitorArea, ObservedTarget, ObservedWindow, clear_pending_seams, inject_pending_elevated,
        inject_pending_foreground, inject_pending_observe, inject_pending_suspend,
        inject_pending_tag,
    };
    use tiler_core::geometry::Rect;
    clear_pending_seams();
    let dir = log_dir("refusal");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = fake_me();
    let areas = vec![MonitorArea {
        device: "DEV1".to_owned(),
        work: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
    }];
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.ensure_device("DEV1");
    let rect = Rect {
        x: 10,
        y: 10,
        w: 200,
        h: 200,
    };
    let key = crate::native_guards::WindowKey {
        hwnd: 0x4201,
        pid: 7201,
        creation: "c-4201".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 7201,
            process_creation: "c-4201".to_owned(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            exe_path: me.exe_path.clone(),
        },
    );
    state
        .member_tags
        .insert(key.clone(), "tag-0x4201".to_owned());
    inject_pending_tag(0x4201, Some("tag-0x4201".to_owned()));
    let observed = vec![ObservedWindow {
        hwnd: 0x4201,
        token: "w-1".to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::measure(rect, rect),
        identity: ObservedTarget {
            hwnd: 0x4201,
            pid: 7201,
            process_creation: "c-4201".to_owned(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: "tag-0x4201".to_owned(),
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
        member_tag: Some("tag-0x4201".to_owned()),
    }];
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
    )
    .expect("seed converges");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    inject_pending_tag(0x4201, Some("tag-0x4201".to_owned()));
    let intent = crate::snapkey::QueuedWorkspaceMigrateIntent {
        direction: tiler_core::directional::Direction::Right,
        edge: crate::snapkey::SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let outcome = workspace_migrate_one(
        &mut state,
        &me,
        &store,
        &dir,
        &fulls,
        &areas,
        &mut [],
        &[],
        "DEV1",
        &intent,
        7,
    );
    assert_eq!(outcome, "migrate-unverified", "no neighbor must refuse");
    let lines = log_lines(&dir);
    assert!(
        lines
            .iter()
            .any(|line| line["event"] == serde_json::json!("world-apply")
                && line["command"] == serde_json::json!("migrate-directional")
                && line["delivery"] == serde_json::json!("rejected")),
        "bounded refusal row logged"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}
