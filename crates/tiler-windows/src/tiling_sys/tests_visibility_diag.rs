#![forbid(unsafe_code)]
// Windows-only bounded normal-level visibility diagnostics (`visibility-
// transition`, `exception-transition`, `workspace-reveal` outcome mapping,
// gesture floating-cause split, monotonic log clock).
//
// Integration shape: every leg drives the production staging/flush seams
// plus the pure outcome/cause/log-line mappings the select and gesture paths
// call, with a pinned hint clock for the throttle. Member assignment uses
// the shared proof-style harness; all HWNDs are synthetic and every native
// visibility read is injected. `trace` stays false throughout: these events
// are normal-level by design.
use super::rmax03_adapter_tests::test_state;
use super::*;
use std::time::{Duration, Instant};

fn vd_facts(cloaked: bool) -> WindowFacts {
    WindowFacts {
        visible: true,
        minimized: false,
        maximized: false,
        cloaked,
        elevated: false,
        shell: false,
        tool_window: false,
        owned: false,
        captionless_fullscreen: false,
        no_activate: false,
        dialog: false,
    }
}

fn vd_member(
    state: &mut TileLoop,
    _output: &str,
    _ws: &str,
    token: &str,
    hwnd: u64,
) -> crate::native_guards::WindowKey {
    let key = crate::native_guards::WindowKey {
        hwnd,
        pid: 7000 + hwnd as u32,
        creation: format!("cvd{hwnd:016x}"),
    };
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-vd".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\VdApp\\vd.exe".to_owned(),
        },
    );
    let tag = format!("vd-tag-{token}");
    state.member_tags.insert(key.clone(), tag.clone());
    state.last_hwnds.insert(key.hwnd);
    inject_pending_tag(key.hwnd, Some(tag));
    key
}

fn vd_observed(token: &str, key: &crate::native_guards::WindowKey, cloaked: bool) -> ObservedWindow {
    ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        visible: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "C:\\Apps\\VdApp\\vd.exe".to_owned(),
            user_sid: "S-1-5-vd".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: vd_facts(cloaked),
    }
}

fn vd_retained(
    key: &crate::native_guards::WindowKey,
    token: &str,
    maximized: bool,
    fullscreen: bool,
    cloaked: bool,
) -> RetainedRow {
    let mut facts = vd_facts(cloaked);
    facts.maximized = maximized;
    RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }),
        maximized,
        fullscreen,
        facts: Some(facts),
    }
}

fn vd_frameless_retained(key: &crate::native_guards::WindowKey, token: &str) -> RetainedRow {
    RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: None,
        maximized: false,
        fullscreen: false,
        facts: None,
    }
}

fn vd_hidden(state: &mut TileLoop, key: &crate::native_guards::WindowKey) {
    let identity = state
        .member_identity
        .get(key)
        .cloned()
        .expect("identity staged");
    state.hidden_claims.insert(
        key.clone(),
        HiddenRecord {
            claim: crate::model::WindowIdentity {
                hwnd: key.hwnd,
                process: identity,
                tag: format!("vd-claim-{}", key.hwnd),
                kind: crate::model::WindowClaimKind::Product,
                show: crate::model::WindowShowState::default(),
            },
            iconic: false,
        },
    );
}

struct Vd {
    state: TileLoop,
    output: String,
    ws: String,
    now: Instant,
}

fn vd_setup() -> Vd {
    clear_test_hints();
    clear_pending_seams();
    clear_visibility();
    let now = Instant::now();
    inject_hint_now(Some(now));
    let mut state = test_state();
    assert!(!state.trace, "visibility diagnostics are normal-level");
    let output = "mon-vd".to_owned();
    state.ensure_device(&output);
    let ws = state
        .active_view(&output)
        .unwrap_or_else(|| "ws-1".to_owned());
    Vd {
        state,
        output,
        ws,
        now,
    }
}

fn vd_teardown() {
    clear_test_hints();
    clear_pending_seams();
    clear_visibility();
}

fn vd_advance(f: &mut Vd, ms: u64) {
    f.now += Duration::from_millis(ms);
    inject_hint_now(Some(f.now));
}

fn vd_text(value: &serde_json::Value) -> String {
    serde_json::to_string(value).expect("serializes")
}

fn vd_redacted(text: &str) {
    for banned in ["hwnd", "exe", "title", "pid", "creation", "sid", "path"] {
        assert!(
            !text.contains(banned),
            "redacted payload must not contain {banned}: {text}"
        );
    }
}

#[test]
fn reveal_outcome_maps_teardown_honestly() {
    use crate::product_hide::ProductTeardown;
    assert_eq!(reveal_outcome(&ProductTeardown::Revealed), "revealed");
    assert_eq!(
        reveal_outcome(&ProductTeardown::AlreadyVisible),
        "already-visible"
    );
    assert_eq!(reveal_outcome(&ProductTeardown::Retired), "retired");
    assert_eq!(reveal_outcome(&ProductTeardown::Uncertain), "uncertain");
    vd_teardown();
}

#[test]
fn reveal_err_outcome_maps_contract_categories() {
    // Every string below is verbatim production vocabulary from
    // `reveal_product_claim_to` / `post_visibility` / `query_recovery`.
    assert_eq!(
        reveal_err_outcome("uncertain: reveal pre-read uncertain: target access-denied"),
        ("uncertain", "reveal-pre-read-access")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal pre-read error: nonce read e"),
        ("uncertain", "reveal-pre-read-nonce")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal pre-read error: target identity e"),
        ("uncertain", "reveal-pre-read")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal held absent: target pid absent"),
        ("uncertain", "reveal-held-absent")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal held error: target held e"),
        ("uncertain", "reveal-held-error")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal post-pid changed"),
        ("uncertain", "reveal-post-pid")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal readback mismatch (kept ledger)"),
        ("uncertain", "reveal-readback-mismatch")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal readback absent: hwnd destroyed"),
        ("uncertain", "reveal-readback-absent")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal readback uncertain: target access-denied"),
        ("uncertain", "reveal-readback-access")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: reveal readback error: target identity e"),
        ("uncertain", "reveal-readback-error")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: visibility not observed"),
        ("uncertain", "reveal-post-unobserved")
    );
    assert_eq!(
        reveal_err_outcome("uncertain: something new"),
        ("uncertain", "reveal-uncertain")
    );
    assert_eq!(
        reveal_err_outcome("refuse: not a product claim"),
        ("refused", "not-product-claim")
    );
    assert_eq!(
        reveal_err_outcome("refuse: ledger owner changed under lease"),
        ("refused", "reveal-refused")
    );
    assert_eq!(
        reveal_err_outcome("absent: hwnd destroyed"),
        ("failed", "reveal-absent")
    );
    assert_eq!(
        reveal_err_outcome("error: visibility post failed"),
        ("failed", "reveal-post-failed")
    );
    assert_eq!(
        reveal_err_outcome("error: brand new failure"),
        ("failed", "reveal-unknown")
    );
    vd_teardown();
}

#[test]
fn reveal_log_line_carries_production_payload() {
    let line = reveal_log_line(425, "act-9", Some("w19"), "revealed", None, None, 12);
    assert_eq!(line["event"], "workspace-reveal");
    assert_eq!(line["tick"], 425);
    assert_eq!(line["correlation"], "act-9");
    assert_eq!(line["window"], "w19");
    assert_eq!(line["outcome"], "revealed");
    assert!(
        line["detail"].is_null(),
        "no detail reads null, never empty"
    );
    assert_eq!(line["elapsed_ms"], 12);
    assert!(line["phase"].is_null(), "ordinary leg carries no phase");
    let refused = reveal_log_line(
        426,
        "act-9",
        None,
        "refused",
        Some("workspace-disabled"),
        None,
        3,
    );
    assert!(refused["window"].is_null());
    assert_eq!(refused["detail"], "workspace-disabled");
    let failed = reveal_log_line(
        427,
        "act-9",
        Some("w78"),
        "uncertain",
        Some("reveal-post-pid"),
        None,
        4,
    );
    assert_eq!(failed["detail"], "reveal-post-pid");
    let rollback = reveal_log_line(
        428,
        "act-9",
        Some("w78"),
        "revealed",
        None,
        Some("rollback"),
        5,
    );
    assert_eq!(
        rollback["phase"], "rollback",
        "recovery leg marks its phase"
    );
    vd_redacted(&vd_text(&line));
    vd_teardown();
}

#[test]
fn gesture_float_cause_truth_table() {
    assert_eq!(
        gesture_float_cause(true, true),
        "engine-exception",
        "Engine exception on a tiled workspace names the exception lane"
    );
    assert_eq!(
        gesture_float_cause(false, false),
        "workspace-floating",
        "clean Engine state on a floating workspace names the workspace mode"
    );
    assert_eq!(
        gesture_float_cause(true, false),
        "engine-exception+workspace-floating",
        "both predicates set names both, never hides one"
    );
    assert_eq!(gesture_float_cause(false, true), "none");
    vd_teardown();
}

#[test]
fn log_json_at_stamps_common_monotonic_time() {
    let path = std::env::temp_dir().join(format!(
        "tiler-vis-mono-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    log_json_at(&path, serde_json::json!({"event": "a"}));
    log_json_at(&path, serde_json::json!({"event": "b", "mono_ms": 7}));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line parses"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0]["mono_ms"].as_u64().is_some(),
        "common monotonic stamp added: {lines:?}"
    );
    assert_eq!(lines[1]["mono_ms"], 7, "existing fields never overwritten");
    vd_teardown();
}

#[test]
fn visibility_emits_at_normal_level_with_evidence() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key_seen = vd_member(&mut f.state, &output, &ws, "wseen", 9101);
    let key_hidden = vd_member(&mut f.state, &output, &ws, "whid", 9102);
    vd_hidden(&mut f.state, &key_hidden);
    inject_visibility(key_hidden.hwnd, false, Some(false));
    f.state.tick = 425;
    let observed = vec![vd_observed("wseen", &key_seen, false)];
    stage_visibility(&mut f.state, &observed, &[], &[]);
    let event = flush_visibility(&mut f.state).expect("transitions emit at normal level");
    assert_eq!(event["event"], "visibility-transition");
    assert_eq!(event["tick"], 425);
    let entries = event["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 2, "one entry per live member");
    let seen = entries
        .iter()
        .find(|e| e["window"] == "wseen")
        .expect("seen entry");
    assert_eq!(seen["state"], "observed");
    assert_eq!(seen["reason"], "eligible");
    assert_eq!(seen["tick"], 425);
    assert_eq!(seen["visible"], true);
    assert_eq!(seen["cloaked"], false);
    assert_eq!(seen["hide_claim"], false);
    assert_eq!(seen["overlay"], "none");
    assert_eq!(seen["history_lost"], false);
    assert!(
        seen["observed_ms"].as_u64().is_some(),
        "capture time rides entries"
    );
    let hidden = entries
        .iter()
        .find(|e| e["window"] == "whid")
        .expect("hidden entry");
    assert_eq!(hidden["state"], "hidden");
    assert_eq!(hidden["hide_claim"], true);
    assert_eq!(hidden["visible"], false, "fenced native read proves hidden");
    assert_eq!(hidden["cloaked"], false);
    vd_redacted(&vd_text(&event));
    vd_teardown();
}

#[test]
fn visibility_reports_native_invisible_for_absent_members() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wgone", 9111);
    inject_visibility(key.hwnd, false, None);
    f.state.tick = 51;
    stage_visibility(&mut f.state, &[], &[], &[]);
    let event = flush_visibility(&mut f.state).expect("absent transition emits");
    let entry = &event["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["state"], "absent");
    assert_eq!(entry["reason"], "not-observed");
    assert_eq!(
        entry["visible"], false,
        "identity-fenced native read, not a guess"
    );
    assert!(
        entry["cloaked"].is_null(),
        "unreadable cloak stays null, never false"
    );
    assert_eq!(entry["overlay"], "unknown");
    vd_teardown();
}

#[test]
fn visibility_reports_unknown_on_identity_break() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wrecycled", 9121);
    inject_pending_tag(key.hwnd, Some("stale-tag".to_owned()));
    inject_visibility(key.hwnd, true, Some(false));
    f.state.tick = 52;
    stage_visibility(&mut f.state, &[], &[], &[]);
    let event = flush_visibility(&mut f.state).expect("absent transition emits");
    let entry = &event["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["state"], "absent");
    assert!(entry["visible"].is_null(), "broken fence reports unknown");
    assert!(entry["cloaked"].is_null(), "no read behind a broken fence");
    vd_teardown();
}

#[test]
fn visibility_keeps_cloak_visible_under_overlay_priority() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key_full = vd_member(&mut f.state, &output, &ws, "wfull", 9131);
    let key_max = vd_member(&mut f.state, &output, &ws, "wmax", 9132);
    f.state.tick = 53;
    let retained = vec![
        vd_retained(&key_full, "wfull", false, true, true),
        vd_retained(&key_max, "wmax", true, false, false),
    ];
    stage_visibility(&mut f.state, &[], &retained, &[]);
    let event = flush_visibility(&mut f.state).expect("overlay transitions emit");
    let entries = event["entries"].as_array().expect("entries array");
    let full = entries
        .iter()
        .find(|e| e["window"] == "wfull")
        .expect("fullscreen entry");
    assert_eq!(full["state"], "retained-fullscreen");
    assert_eq!(full["overlay"], "fullscreen");
    assert_eq!(full["cloaked"], true, "cloak survives fullscreen priority");
    assert_eq!(full["visible"], true);
    let max = entries
        .iter()
        .find(|e| e["window"] == "wmax")
        .expect("maximized entry");
    assert_eq!(max["state"], "retained-maximized");
    assert_eq!(max["overlay"], "maximized");
    vd_teardown();
}

#[test]
fn visibility_uses_skip_reason_for_frameless_retained_rows() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wunread", 9141);
    f.state.tick = 54;
    let retained = vec![vd_frameless_retained(&key, "wunread")];
    let skipped = vec![("wunread".to_owned(), "unreadable".to_owned())];
    stage_visibility(&mut f.state, &[], &retained, &skipped);
    let event = flush_visibility(&mut f.state).expect("transition emits");
    let entry = &event["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["state"], "absent-unreadable");
    assert_eq!(
        entry["reason"], "unreadable",
        "skip reason survives frameless rows"
    );
    vd_teardown();
}

#[test]
fn visibility_dedupes_steady_state() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wsteady", 9301);
    f.state.tick = 3;
    let observed = vec![vd_observed("wsteady", &key, false)];
    stage_visibility(&mut f.state, &observed, &[], &[]);
    assert!(
        flush_visibility(&mut f.state).is_some(),
        "first snapshot emits"
    );
    f.state.tick = 4;
    let observed = vec![vd_observed("wsteady", &key, false)];
    stage_visibility(&mut f.state, &observed, &[], &[]);
    assert!(
        flush_visibility(&mut f.state).is_none(),
        "steady state stays quiet"
    );
    vd_teardown();
}

#[test]
fn visibility_throttle_coalesces_with_suppressed_counts() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key_a = vd_member(&mut f.state, &output, &ws, "wa", 9401);
    let _key_b = vd_member(&mut f.state, &output, &ws, "wb", 9402);
    f.state.tick = 10;
    stage_visibility(&mut f.state, &[vd_observed("wa", &key_a, false)], &[], &[]);
    stage_visibility(
        &mut f.state,
        &[vd_observed("wa", &key_a, false)],
        &[],
        &[("wb".to_owned(), "minimized".to_owned())],
    );
    let first = flush_visibility(&mut f.state).expect("first batch emits");
    assert_eq!(first["coalesced"], 0);
    f.state.tick = 11;
    stage_visibility(
        &mut f.state,
        &[vd_observed("wa", &key_a, true)],
        &[],
        &[("wb".to_owned(), "minimized".to_owned())],
    );
    assert!(
        flush_visibility(&mut f.state).is_none(),
        "throttled round stays quiet"
    );
    vd_advance(&mut f, 1100);
    let second = flush_visibility(&mut f.state).expect("interval expiry emits");
    assert_eq!(second["coalesced"], 1, "throttled token counts once");
    let entries = second["entries"].as_array().expect("entries array");
    assert!(
        entries
            .iter()
            .any(|e| e["window"] == "wa" && e["state"] == "observed-cloaked")
    );
    vd_teardown();
}

#[test]
fn visibility_marks_lost_history_on_out_and_back() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wflip", 9411);
    f.state.tick = 60;
    stage_visibility(&mut f.state, &[vd_observed("wflip", &key, false)], &[], &[]);
    assert!(flush_visibility(&mut f.state).is_some(), "baseline emits");
    // Out and back inside one throttle interval: the cloaked intermediate
    // never emits on its own, and the return matches the baseline exactly.
    f.state.tick = 61;
    stage_visibility(&mut f.state, &[vd_observed("wflip", &key, true)], &[], &[]);
    assert!(
        flush_visibility(&mut f.state).is_none(),
        "out-leg throttled"
    );
    stage_visibility(&mut f.state, &[vd_observed("wflip", &key, false)], &[], &[]);
    assert!(
        flush_visibility(&mut f.state).is_none(),
        "back-leg throttled"
    );
    vd_advance(&mut f, 1100);
    let event = flush_visibility(&mut f.state).expect("lost history still emits");
    let entry = &event["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["window"], "wflip");
    assert_eq!(entry["state"], "observed", "latest sample wins");
    assert_eq!(
        entry["history_lost"], true,
        "intermediate explicitly marked lost"
    );
    assert_eq!(event["coalesced"], 1);
    vd_teardown();
}

#[test]
fn visibility_caps_entries_with_honest_omitted() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let mut observed = Vec::new();
    for i in 0..20 {
        let token = format!("wcap{i:02}");
        let key = vd_member(&mut f.state, &output, &ws, &token, 9500 + i as u64);
        observed.push(vd_observed(&token, &key, false));
    }
    f.state.tick = 20;
    stage_visibility(&mut f.state, &observed, &[], &[]);
    let event = flush_visibility(&mut f.state).expect("batch emits");
    assert_eq!(event["entries"].as_array().expect("entries").len(), 16);
    assert_eq!(
        event["omitted"], 4,
        "entries past the cap stay pending honestly"
    );
    vd_teardown();
}

#[test]
fn exception_lane_enter_exit_with_reasons() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wfloat", 9601);
    f.state.tick = 30;
    stage_exceptions(&mut f.state);
    assert!(
        flush_exceptions(&mut f.state).is_none(),
        "tiled baseline stays quiet"
    );
    f.state.floated.insert(key.clone());
    vd_advance(&mut f, 1100);
    f.state.tick = 31;
    stage_exceptions(&mut f.state);
    let entered = flush_exceptions(&mut f.state).expect("lane entry emits");
    assert_eq!(entered["event"], "exception-transition");
    let entry = &entered["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["window"], "wfloat");
    assert_eq!(entry["transition"], "entered");
    assert_eq!(entry["reason"], "intentional-float-lane");
    assert!(
        entry["lanes"]
            .as_array()
            .expect("lanes")
            .iter()
            .any(|l| l == "intentional-float")
    );
    assert_eq!(entry["history_lost"], false);
    assert!(entry["observed_ms"].as_u64().is_some());
    f.state.floated.remove(&key);
    f.state.tick = 32;
    stage_exceptions(&mut f.state);
    assert!(
        flush_exceptions(&mut f.state).is_none(),
        "throttled exit stays quiet"
    );
    vd_advance(&mut f, 1100);
    let exited = flush_exceptions(&mut f.state).expect("lane exit emits after interval");
    let entry = &exited["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["transition"], "exited");
    assert_eq!(entry["reason"], "tiled");
    vd_redacted(&vd_text(&exited));
    vd_teardown();
}

#[test]
fn exception_marks_lost_history_on_out_and_back() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wexc", 9611);
    f.state.floated.insert(key.clone());
    f.state.tick = 70;
    stage_exceptions(&mut f.state);
    assert!(
        flush_exceptions(&mut f.state).is_some(),
        "lane baseline emits"
    );
    // Exit and re-enter inside one throttle interval: the exit never emits
    // on its own, and the return matches the emitted baseline exactly.
    f.state.floated.remove(&key);
    f.state.tick = 71;
    stage_exceptions(&mut f.state);
    assert!(
        flush_exceptions(&mut f.state).is_none(),
        "exit-leg throttled"
    );
    f.state.floated.insert(key.clone());
    stage_exceptions(&mut f.state);
    assert!(
        flush_exceptions(&mut f.state).is_none(),
        "re-enter-leg throttled"
    );
    vd_advance(&mut f, 1100);
    let event = flush_exceptions(&mut f.state).expect("lost history still emits");
    let entry = &event["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["window"], "wexc");
    assert_eq!(
        entry["transition"], "unchanged",
        "no net lane change keeps an explicit label"
    );
    assert_eq!(entry["history_lost"], true, "exit explicitly marked lost");
    assert!(
        entry["lanes"]
            .as_array()
            .expect("lanes")
            .iter()
            .any(|l| l == "intentional-float")
    );
    vd_teardown();
}

#[test]
fn exception_names_sticky_and_born_lanes() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key_sticky = vd_member(&mut f.state, &output, &ws, "wsticky", 9701);
    let key_born = vd_member(&mut f.state, &output, &ws, "wborn", 9702);
    f.state.sticky.insert(key_sticky.clone(), false);
    f.state.born_fullscreen.insert(key_born.clone());
    f.state.tick = 40;
    stage_exceptions(&mut f.state);
    let event = flush_exceptions(&mut f.state).expect("lane entries emit");
    let entries = event["entries"].as_array().expect("entries array");
    let sticky = entries
        .iter()
        .find(|e| e["window"] == "wsticky")
        .expect("sticky entry");
    assert_eq!(sticky["reason"], "sticky-lane");
    let born = entries
        .iter()
        .find(|e| e["window"] == "wborn")
        .expect("born entry");
    assert_eq!(born["reason"], "born-hold-lane");
    vd_teardown();
}

#[test]
fn drop_visibility_sample_prunes_dead_members() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wgone", 9801);
    f.state.tick = 50;
    stage_visibility(&mut f.state, &[vd_observed("wgone", &key, false)], &[], &[]);
    drop_visibility_sample(&mut f.state, "wgone");
    assert!(
        flush_visibility(&mut f.state).is_none(),
        "dropped sample never emits"
    );
    vd_teardown();
}

fn vd_ctx(correlation: &str) -> ActionCtx {
    ActionCtx {
        correlation: correlation.to_owned(),
        tick: 99,
        queued_at: Instant::now(),
        start: Instant::now(),
    }
}

#[test]
fn shared_hide_helper_bounds_and_summarizes() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wh", 9851);
    f.state.tick = 77;
    let ctx = vd_ctx("act-77");
    let mut logged = 0;
    note_hide_diag(&mut f.state, &ctx, &key, "hidden", None, false, &mut logged);
    assert_eq!(logged, 1);
    let mut capped = MAX_HIDE_DIAG_PER_SELECT + 5;
    note_hide_diag(&mut f.state, &ctx, &key, "hidden", None, false, &mut capped);
    assert_eq!(
        capped,
        MAX_HIDE_DIAG_PER_SELECT + 5,
        "shared hide helper honors the hide cap"
    );
    let full = omission_summary("workspace-hide-summary", 77, "act-77", None, 70, 64);
    let summary = full.expect("omitted attempts summarize");
    assert_eq!(summary["event"], "workspace-hide-summary");
    assert_eq!(summary["attempted"], 70);
    assert_eq!(summary["logged"], 64);
    assert_eq!(summary["omitted"], 6);
    assert!(summary["phase"].is_null());
    assert!(
        omission_summary("workspace-hide-summary", 77, "act-77", None, 3, 3).is_none(),
        "complete legs stay silent"
    );
    let rollback = omission_summary(
        "workspace-reveal-summary",
        77,
        "act-77",
        Some("rollback"),
        5,
        3,
    )
    .expect("rollback omissions summarize");
    assert_eq!(rollback["phase"], "rollback");
    assert_eq!(rollback["omitted"], 2);
    vd_teardown();
}

#[test]
fn shared_reveal_helper_bounds_through_one_cap() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wr", 9861);
    f.state.tick = 78;
    f.state.log_path = std::env::temp_dir().join(format!(
        "tiler-vis-cap-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let ctx = vd_ctx("act-78");
    let mut logged = 0;
    for _ in 0..REVEAL_DIAG_PER_SELECT + 10 {
        note_reveal_diag(
            &mut f.state,
            &ctx,
            &key,
            "revealed",
            None,
            None,
            &mut logged,
        );
    }
    assert_eq!(logged, REVEAL_DIAG_PER_SELECT, "one shared reveal cap");
    let text = std::fs::read_to_string(&f.state.log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&f.state.log_path);
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), REVEAL_DIAG_PER_SELECT);
    for line in lines {
        let value: serde_json::Value = serde_json::from_str(line).expect("log line parses");
        assert_eq!(value["event"], "workspace-reveal");
        assert_eq!(value["window"], "wr");
        assert!(
            value["mono_ms"].as_u64().is_some(),
            "shared lines carry common time"
        );
    }
    vd_teardown();
}

#[test]
fn exception_labels_net_change_honestly() {
    let mut f = vd_setup();
    let output = f.output.clone();
    let ws = f.ws.clone();
    let key = vd_member(&mut f.state, &output, &ws, "wlab", 9871);
    // Entered: baseline zero to nonzero lanes.
    f.state.floated.insert(key.clone());
    f.state.tick = 80;
    stage_exceptions(&mut f.state);
    let entered = flush_exceptions(&mut f.state).expect("entry emits");
    assert_eq!(
        entered["entries"].as_array().expect("entries")[0]["transition"],
        "entered"
    );
    // Changed: nonzero lanes to different nonzero lanes.
    vd_advance(&mut f, 1100);
    f.state.sticky.insert(key.clone(), false);
    f.state.tick = 81;
    stage_exceptions(&mut f.state);
    let changed = flush_exceptions(&mut f.state).expect("lane change emits");
    let entry = &changed["entries"].as_array().expect("entries")[0];
    assert_eq!(entry["transition"], "changed");
    assert_eq!(entry["history_lost"], false);
    assert_eq!(entry["reason"], "sticky-lane");
    vd_teardown();
}

#[test]
fn rehide_err_outcome_maps_rehide_vocabulary() {
    // Every string below mirrors the production `rehide_known_claim` /
    // `post_visibility` / `query_recovery` vocabulary, including raw
    // `query_recovery` passthrough errors that must collapse to categories.
    assert_eq!(
        rehide_err_outcome("uncertain: rehide pre-read uncertain: target access-denied pid 1234"),
        ("uncertain", "rehide-pre-read")
    );
    assert_eq!(
        rehide_err_outcome("uncertain: rehide held error: target held e"),
        ("uncertain", "rehide-held")
    );
    assert_eq!(
        rehide_err_outcome("uncertain: rehide post-pid changed"),
        ("uncertain", "rehide-post-pid")
    );
    assert_eq!(
        rehide_err_outcome("uncertain: rehide readback mismatch (kept ledger)"),
        ("uncertain", "rehide-readback-mismatch")
    );
    assert_eq!(
        rehide_err_outcome("uncertain: visibility not observed"),
        ("uncertain", "rehide-post-unobserved")
    );
    assert_eq!(
        rehide_err_outcome("uncertain: reveal readback error: target identity e"),
        ("uncertain", "rehide-uncertain")
    );
    assert_eq!(
        rehide_err_outcome("refuse: not a product claim"),
        ("refused", "rehide-refused")
    );
    assert_eq!(
        rehide_err_outcome("absent: hwnd destroyed"),
        ("failed", "rehide-absent")
    );
    assert_eq!(
        rehide_err_outcome("error: visibility post failed"),
        ("failed", "rehide-post-failed")
    );
    assert_eq!(
        rehide_err_outcome("error: target held e"),
        ("failed", "rehide-error")
    );
    assert_eq!(
        rehide_err_outcome("brand new failure shape"),
        ("failed", "rehide-unknown")
    );
    // No raw error text survives: identities and numbers never reach the log.
    let (_, detail) =
        rehide_err_outcome("uncertain: rehide pre-read uncertain: target access-denied pid 1234");
    assert_eq!(detail, "rehide-pre-read");
    assert!(!detail.contains("1234"), "raw identity never leaks");
    vd_teardown();
}

#[test]
fn rehide_sweep_counts_honestly_with_capped_samples() {
    let mut f = vd_setup();
    f.state.tick = 91;
    note_rehide_diag(&mut f.state, Some("w1"), "corrected", "rehidden");
    note_rehide_diag(&mut f.state, Some("w1"), "corrected", "rehidden");
    note_rehide_diag(&mut f.state, None, "uncertain", "rehide-pre-read");
    note_rehide_diag(&mut f.state, Some(""), "failed", "rehide-post-failed");
    let sweep = flush_rehide_diag(&mut f.state).expect("pending outcomes emit");
    assert_eq!(sweep["event"], "rehide-sweep");
    assert_eq!(sweep["tick"], 91);
    let buckets = sweep["buckets"].as_array().expect("buckets array");
    assert_eq!(buckets.len(), 3, "one bucket per outcome bucket");
    assert_eq!(buckets[0]["outcome"], "corrected");
    assert_eq!(buckets[0]["detail"], "rehidden");
    assert_eq!(buckets[0]["count"], 2);
    assert!(
        buckets
            .windows(2)
            .all(|pair| pair[0]["outcome"].as_str() <= pair[1]["outcome"].as_str()),
        "deterministic bucket order: {buckets:?}"
    );
    let entries = sweep["entries"].as_array().expect("entries array");
    assert_eq!(
        entries.len(),
        1,
        "same window lists once; unknowns never list"
    );
    assert_eq!(entries[0]["window"], "w1");
    assert_eq!(entries[0]["outcome"], "corrected");
    assert_eq!(sweep["omitted"], 3, "four instances, one window listed");
    assert_eq!(
        entries.len() + sweep["omitted"].as_u64().expect("omitted") as usize,
        4,
        "listed plus omitted equals noted instances"
    );
    vd_redacted(&vd_text(&sweep));
    assert!(
        flush_rehide_diag(&mut f.state).is_none(),
        "drained sweep stays quiet"
    );
    vd_teardown();
}

#[test]
fn rehide_sweep_bounds_flood_to_one_line_per_second() {
    let mut f = vd_setup();
    f.state.tick = 92;
    let legs = [
        ("corrected", "rehidden"),
        ("retired", "revealed"),
        ("retired", "already-visible"),
        ("uncertain", "rehide-pre-read"),
        ("failed", "rehide-post-failed"),
    ];
    for (leg, &(outcome, detail)) in legs.iter().enumerate() {
        for i in 0..5 {
            let token = format!("wf{leg}{i}");
            note_rehide_diag(&mut f.state, Some(token.as_str()), outcome, detail);
        }
    }
    let sweep = flush_rehide_diag(&mut f.state).expect("first sweep emits");
    let entries = sweep["entries"].as_array().expect("entries array");
    assert_eq!(
        entries.len(),
        REHIDE_DIAG_MAX_ENTRIES,
        "windows listed through one shared cap"
    );
    let counted: usize = sweep["buckets"]
        .as_array()
        .expect("buckets")
        .iter()
        .map(|bucket| bucket["count"].as_u64().expect("count") as usize)
        .sum();
    assert_eq!(counted, 25, "every instance counted");
    assert_eq!(
        sweep["omitted"],
        25 - REHIDE_DIAG_MAX_ENTRIES,
        "unlisted instances stay honest"
    );
    assert_eq!(
        entries.len() + sweep["omitted"].as_u64().expect("omitted") as usize,
        counted,
        "listed plus omitted equals bucket-counted instances"
    );
    assert!(
        flush_rehide_diag(&mut f.state).is_none(),
        "drained sweep stays quiet"
    );
    // Persistent drift inside the same second accumulates silently.
    note_rehide_diag(&mut f.state, Some("wf00"), "corrected", "rehidden");
    assert!(
        flush_rehide_diag(&mut f.state).is_none(),
        "throttled round emits nothing"
    );
    vd_advance(&mut f, 1100);
    let second = flush_rehide_diag(&mut f.state).expect("interval expiry emits");
    assert_eq!(second["buckets"].as_array().expect("buckets").len(), 1);
    assert_eq!(second["buckets"][0]["count"], 1);
    assert_eq!(second["omitted"], 0);
    vd_teardown();
}

#[test]
fn rehide_retired_detail_names_phase_without_raw_text() {
    assert_eq!(
        rehide_retired_detail("uncertain"),
        "retired-reveal-uncertain"
    );
    assert_eq!(rehide_retired_detail("refused"), "retired-reveal-refused");
    assert_eq!(rehide_retired_detail("failed"), "retired-reveal-failed");
    assert_eq!(
        rehide_retired_detail("anything-else"),
        "retired-reveal-failed",
        "unknown outcomes fail closed, never a verified retirement"
    );
    vd_teardown();
}


#[test]
fn sweep_reveal_lines_pin_maintenance_and_migrate_phases() {
    // Audit retirements reuse the shared reveal payload under the maintenance
    // sweep correlation with zero elapsed (no action clock); migration legs
    // ride the action correlation in the migrate phase.
    let audit = reveal_log_line(
        77,
        "maintenance",
        Some("w9"),
        "retired",
        Some("already-visible"),
        Some("audit"),
        0,
    );
    assert_eq!(audit["event"], "workspace-reveal");
    assert_eq!(audit["tick"], 77);
    assert_eq!(audit["correlation"], "maintenance");
    assert_eq!(audit["window"], "w9");
    assert_eq!(audit["outcome"], "retired");
    assert_eq!(audit["detail"], "already-visible");
    assert_eq!(audit["phase"], "audit");
    assert_eq!(audit["elapsed_ms"], 0);
    let unknown = reveal_log_line(
        78,
        "maintenance",
        None,
        "retired",
        Some("retired"),
        Some("audit"),
        0,
    );
    assert!(unknown["window"].is_null(), "unknown token reads null");
    let migrate = reveal_log_line(
        79,
        "act-3",
        Some("w10"),
        "revealed",
        None,
        Some("migrate"),
        5,
    );
    assert_eq!(migrate["correlation"], "act-3");
    assert_eq!(migrate["phase"], "migrate");
    assert!(migrate["detail"].is_null());
    vd_redacted(&vd_text(&audit));
    vd_redacted(&vd_text(&migrate));
    vd_teardown();
}
