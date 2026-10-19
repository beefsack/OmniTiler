#![forbid(unsafe_code)]
//! Offline contract tests for the experimental cloak spike: parsers, the
//! hard-coded build fence, baseline/restoration gates, exact-HWND event
//! filtering, JSONL log / stop-file gates, line shapes, and the
//! verified-write sequencer with injected operation seams (failed AddTab
//! still uncloaks, expired targets forbid restore writes, post-cloak errors
//! trigger restore, the uncloak call always runs). No Win32 calls, no live
//! effects.

use std::collections::{HashMap, VecDeque};

use tiler_windows::cloak::{
    CONTROLLER_MAX_SECONDS, CONTROLLER_MIN_SECONDS, CloakOps, CloakStateReport,
    EVENT_COMPLETENESS_NOTE, EXPERIMENTAL_ARCH, EXPERIMENTAL_BUILD, EventLine, StartLine,
    VerifyFault, WriteOp, cloak_part, cloak_reason, event_is_relevant, event_line, hook_event_kind,
    obligation_line, parse_cloak_args, parse_observe_args, parse_uncloak_args,
    restoration_mismatches, restore_part, start_line, step_line, stop_file_armed, stop_requested,
    supports_build, validate_baseline, verified_write,
};
use tiler_windows::model::ProcessIdentity;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn cloaked_state() -> CloakStateReport {
    CloakStateReport::new(
        0x1234,
        true,
        false,
        Some(2),
        Some([10, 20, 310, 220]),
        0x9999,
    )
}

fn uncloaked_state() -> CloakStateReport {
    CloakStateReport::new(
        0x1234,
        true,
        false,
        Some(0),
        Some([10, 20, 310, 220]),
        0x9999,
    )
}

fn identity() -> ProcessIdentity {
    ProcessIdentity {
        pid: 4242,
        process_creation: "creation".to_owned(),
        user_sid: "sid".to_owned(),
        session_id: 1,
        exe_path: "exe".to_owned(),
    }
}

/// Scripted seam: programmed verify states, verify blackout from a call
/// index on, per-op write failures, scripted log persistence, and full
/// call/log recording.
struct FakeOps {
    states: VecDeque<CloakStateReport>,
    last: CloakStateReport,
    verify_calls: usize,
    fail_verify_from: Option<usize>,
    write_fail: HashMap<&'static str, String>,
    log_ok: bool,
    calls: Vec<String>,
    logs: Vec<serde_json::Value>,
}

impl FakeOps {
    fn new(last: CloakStateReport) -> Self {
        Self {
            states: VecDeque::new(),
            last,
            verify_calls: 0,
            fail_verify_from: None,
            write_fail: HashMap::new(),
            log_ok: true,
            calls: Vec::new(),
            logs: Vec::new(),
        }
    }

    fn scripted(states: Vec<CloakStateReport>) -> Self {
        let last = states.last().cloned().unwrap_or_else(uncloaked_state);
        Self {
            states: states.into(),
            ..Self::new(last)
        }
    }

    fn log_kinds(&self) -> Vec<String> {
        self.logs
            .iter()
            .map(|v| {
                let line = v.get("line").and_then(|l| l.as_str()).unwrap_or("?");
                let op = v
                    .get("op")
                    .and_then(|o| o.as_str())
                    .or_else(|| v.get("kind").and_then(|k| k.as_str()))
                    .unwrap_or("");
                format!("{line}:{op}")
            })
            .collect()
    }
}

impl CloakOps for FakeOps {
    fn verify(&mut self) -> Result<CloakStateReport, VerifyFault> {
        self.calls.push("verify".to_owned());
        if self
            .fail_verify_from
            .is_some_and(|n| self.verify_calls >= n)
        {
            return Err(VerifyFault("helper identity changed".to_owned()));
        }
        self.verify_calls += 1;
        Ok(self.states.pop_front().unwrap_or_else(|| self.last.clone()))
    }

    fn write(&mut self, op: WriteOp) -> Result<(), String> {
        self.calls.push(format!("write:{}", op.name()));
        if let Some(msg) = self.write_fail.get(op.name()) {
            return Err(msg.clone());
        }
        Ok(())
    }

    fn log(&mut self, line: serde_json::Value) -> bool {
        let kind = line
            .get("line")
            .and_then(|l| l.as_str())
            .unwrap_or("?")
            .to_owned();
        let op = line
            .get("op")
            .and_then(|o| o.as_str())
            .or_else(|| line.get("kind").and_then(|k| k.as_str()))
            .unwrap_or("")
            .to_owned();
        self.calls.push(format!("log:{kind}:{op}"));
        self.logs.push(line);
        self.log_ok
    }
}

#[test]
fn experimental_fence_is_hard_coded() {
    assert_eq!(EXPERIMENTAL_BUILD, 26300);
    assert_eq!(EXPERIMENTAL_ARCH, "x86_64");
    assert_eq!((CONTROLLER_MIN_SECONDS, CONTROLLER_MAX_SECONDS), (1, 120));
    assert!(supports_build(26300, "x86_64"));
    assert!(!supports_build(26100, "x86_64"));
    assert!(!supports_build(26300, "aarch64"));
    assert!(!supports_build(26300, ""));
}

#[test]
fn baseline_requires_visible_uncloaked_noniconic_and_geometry() {
    let rect = Some([10, 20, 310, 220]);
    assert_eq!(validate_baseline(true, false, Some(0), rect), Ok(0));
    for mask in [1u32, 2, 4, 7, 8] {
        assert!(
            validate_baseline(true, false, Some(mask), rect).is_err(),
            "mask={mask} must refuse"
        );
    }
    assert!(validate_baseline(false, false, Some(0), rect).is_err());
    assert!(validate_baseline(true, true, Some(0), rect).is_err());
    assert!(validate_baseline(true, false, None, rect).is_err());
    // Geometry failure is a refusal, never a fake rect.
    assert!(validate_baseline(true, false, Some(0), None).is_err());
}

#[test]
fn restoration_flags_geometry_visibility_and_mask() {
    let rect = Some([1, 2, 3, 4]);
    assert!(restoration_mismatches(rect, rect, true, true, Some(0)).is_empty());
    assert!(!restoration_mismatches(rect, Some([1, 2, 3, 5]), true, true, Some(0)).is_empty());
    assert!(!restoration_mismatches(rect, rect, true, false, Some(0)).is_empty());
    assert!(!restoration_mismatches(rect, rect, true, true, Some(2)).is_empty());
    assert!(!restoration_mismatches(rect, rect, true, true, None).is_empty());
    // Unreadable geometry on either side is a mismatch, never an assumed match.
    assert!(!restoration_mismatches(None, rect, true, true, Some(0)).is_empty());
    assert!(!restoration_mismatches(rect, None, true, true, Some(0)).is_empty());
}

#[test]
fn event_log_is_exact_hwnd_only() {
    assert!(event_is_relevant(0x1234, 0x1234));
    assert!(!event_is_relevant(0x1235, 0x1234));
    assert!(!event_is_relevant(0, 0));
    assert!(!event_is_relevant(0x1234, 0));
}

#[test]
fn hook_event_kinds_cover_only_foreground_and_cloak_transitions() {
    assert_eq!(hook_event_kind(0x0003), "foreground");
    assert_eq!(hook_event_kind(0x8017), "cloaked");
    assert_eq!(hook_event_kind(0x8018), "uncloaked");
    assert_eq!(hook_event_kind(0x8002), "winevent");
}

#[test]
fn cloak_mask_reason_vocabulary_is_closed() {
    for (mask, want) in [
        (0, "none"),
        (1, "app"),
        (2, "shell"),
        (3, "app|shell"),
        (4, "inherited"),
        (7, "app|shell|inherited"),
    ] {
        assert_eq!(cloak_reason(mask), want, "mask={mask}");
    }
    assert_eq!(cloak_reason(8), "unknown-bits");
    assert_eq!(cloak_reason(u32::MAX), "unknown-bits");
}

#[test]
fn controller_parser_requires_stop_file_and_log() {
    let base = ["99", "--tag", "t", "--seconds", "10"];
    // Both mandatory paths missing.
    assert!(parse_cloak_args(&strings(&base)).is_err());
    let mut no_log: Vec<String> = strings(&base);
    no_log.extend(strings(&["--stop-file", "s"]));
    assert!(parse_cloak_args(&no_log).is_err());
    let mut no_stop: Vec<String> = strings(&base);
    no_stop.extend(strings(&["--log", "l"]));
    assert!(parse_cloak_args(&no_stop).is_err());
    let mut full: Vec<String> = strings(&base);
    full.extend(strings(&["--stop-file", "s", "--log", "l"]));
    let ok = parse_cloak_args(&full).expect("parsed");
    assert_eq!((ok.hwnd, ok.seconds), (99, 10));
    assert!(!ok.delete_tab);
    assert_eq!(ok.stop_file.to_str(), Some("s"));
    assert_eq!(ok.log.to_str(), Some("l"));
    let mut paired = full.clone();
    paired.push("--delete-tab".to_owned());
    assert!(parse_cloak_args(&paired).expect("paired").delete_tab);
    assert!(
        parse_cloak_args(&strings(&[
            "0",
            "--tag",
            "t",
            "--seconds",
            "10",
            "--stop-file",
            "s",
            "--log",
            "l"
        ]))
        .is_err()
    );
    assert!(
        parse_cloak_args(&strings(&[
            "99",
            "--tag",
            "t",
            "--seconds",
            "0",
            "--stop-file",
            "s",
            "--log",
            "l"
        ]))
        .is_err()
    );
    assert!(
        parse_cloak_args(&strings(&[
            "99",
            "--tag",
            "t",
            "--seconds",
            "121",
            "--stop-file",
            "s",
            "--log",
            "l"
        ]))
        .is_err()
    );
}

#[test]
fn recovery_parser_pairs_add_tab_explicitly() {
    let bare = parse_uncloak_args(&strings(&["99", "--tag", "t"])).expect("parsed");
    assert!(!bare.added_tab);
    let paired =
        parse_uncloak_args(&strings(&["99", "--tag", "t", "--added-tab"])).expect("paired");
    assert!(paired.added_tab);
    assert!(parse_uncloak_args(&strings(&["99"])).is_err());
    assert!(parse_uncloak_args(&strings(&["99", "--tag", "t", "--added-tab", "x"])).is_err());
}

#[test]
fn observer_parser_requires_stop_file_and_log() {
    let base = ["99", "--tag", "t", "--seconds", "5"];
    assert!(parse_observe_args(&strings(&base)).is_err());
    let mut full: Vec<String> = strings(&base);
    full.extend(strings(&["--stop-file", "s", "--log", "l"]));
    let ok = parse_observe_args(&full).expect("parsed");
    assert_eq!((ok.hwnd, ok.seconds), (99, 5));
    assert!(
        parse_observe_args(&strings(&[
            "99",
            "--tag",
            "t",
            "--seconds",
            "0",
            "--stop-file",
            "s",
            "--log",
            "l"
        ]))
        .is_err()
    );
}

#[test]
fn observation_contract_disclaims_completeness() {
    assert!(
        EVENT_COMPLETENESS_NOTE.contains("never proves completeness"),
        "hook and poll absence must never claim event completeness"
    );
}

fn temp_run_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omnitiler-cloak-test-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn stop_file_must_not_preexist_and_polls_existence() {
    let dir = temp_run_dir("stop");
    let missing = dir.join("stop.txt");
    assert!(stop_file_armed(&missing).is_ok());
    assert!(!stop_requested(&missing));
    std::fs::write(&missing, b"stop").expect("write stop");
    assert!(stop_file_armed(&missing).is_err());
    assert!(stop_requested(&missing));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn jsonl_log_is_create_new_and_flushed() {
    use tiler_windows::cloak::JsonlLog;
    let dir = temp_run_dir("log");
    let path = dir.join("run.jsonl");
    let mut log = JsonlLog::create(&path).expect("fresh log");
    assert!(JsonlLog::create(&path).is_err(), "preexisting log refuses");
    assert!(!log.failed);
    log.append(&serde_json::json!({"line": "start"}));
    assert!(!log.failed);
    let text = std::fs::read_to_string(&path).expect("read log");
    assert_eq!(text.trim_end(), r#"{"line":"start"}"#);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn start_obligation_and_event_lines_carry_the_contract() {
    let start = start_line(StartLine {
        controller: &identity(),
        target: &identity(),
        hwnd: 0x1234,
        tag: "tag",
        baseline: &uncloaked_state(),
        build: 26300,
        variant: "cloak",
        intended_ops: &["cloak", "uncloak"],
        recovery: "uncloak 4660 --tag tag [--added-tab]",
    });
    assert_eq!(start.get("line").and_then(|v| v.as_str()), Some("start"));
    assert!(start.get("controller").is_some());
    assert!(start.get("target").is_some());
    assert!(start.get("baseline").is_some());
    assert_eq!(
        start.get("recovery").and_then(|v| v.as_str()),
        Some("uncloak 4660 --tag tag [--added-tab]")
    );
    let obligation = obligation_line("add-tab", "owed");
    assert_eq!(
        obligation.get("line").and_then(|v| v.as_str()),
        Some("obligation")
    );
    let event = event_line(EventLine {
        seq: 7,
        elapsed_ms: 1500,
        kind: "cloaked",
        event_id: Some(0x8017),
        os_time: Some(4242),
        state: Some(&cloaked_state()),
        verified: true,
        detail: "",
    });
    assert_eq!(event.get("os_time").and_then(|v| v.as_u64()), Some(4242));
    assert_eq!(event.get("event_id").and_then(|v| v.as_u64()), Some(0x8017));
    let step = step_line("cloak", true, Some(&cloaked_state()), "after", "");
    assert_eq!(step.get("op").and_then(|v| v.as_str()), Some("cloak"));
    assert_eq!(step.get("phase").and_then(|v| v.as_str()), Some("after"));
    assert!(step.get("state").is_some());
}

#[test]
fn failed_delete_tab_does_not_skip_uncloak() {
    // before/after states for cloak, delete-tab, uncloak, add-tab.
    let mut ops = FakeOps::scripted(vec![
        uncloaked_state(),
        cloaked_state(),
        cloaked_state(),
        cloaked_state(),
        uncloaked_state(),
        uncloaked_state(),
        uncloaked_state(),
    ]);
    ops.write_fail
        .insert("delete-tab", "taskbar refused".to_owned());
    let part = cloak_part(&mut ops, true);
    assert!(part.mutation_attempted());
    assert!(part.tab_obligation);
    assert_eq!(part.errors().len(), 1);
    assert!(part.errors()[0].contains("delete-tab"));
    let restore = restore_part(&mut ops, part.tab_obligation);
    // Uncloak runs before AddTab even though DeleteTab failed.
    let uncloak_at = ops
        .calls
        .iter()
        .position(|c| c == "write:uncloak")
        .expect("uncloak attempted");
    let add_at = ops
        .calls
        .iter()
        .position(|c| c == "write:add-tab")
        .expect("add-tab attempted");
    assert!(uncloak_at < add_at);
    assert!(restore.errors().is_empty());
}

#[test]
fn failed_cloak_does_not_delete_tab() {
    let mut ops = FakeOps::scripted(vec![uncloaked_state(), cloaked_state(), uncloaked_state()]);
    ops.write_fail
        .insert("cloak", "SetCloak failed code=-1".to_owned());
    let part = cloak_part(&mut ops, true);
    // The failed call may still have mutated state, so restoration is owed.
    assert!(part.mutation_attempted());
    // But no new hide mutation follows a failed cloak: no obligation, no
    // DeleteTab attempt at all.
    assert!(!part.tab_obligation);
    assert!(part.delete_tab.is_none());
    assert!(
        !ops.calls.iter().any(|c| c == "write:delete-tab"),
        "no pairing after failed cloak: {:?}",
        ops.calls
    );
    assert!(
        !ops.calls.iter().any(|c| c == "log:obligation:add-tab"),
        "no obligation without an attempt: {:?}",
        ops.calls
    );
    assert_eq!(part.errors().len(), 1);
}

#[test]
fn unreadable_cloak_skips_pairing() {
    let unreadable =
        CloakStateReport::new(0x1234, true, false, None, Some([10, 20, 310, 220]), 0x9999);
    let mut ops = FakeOps::scripted(vec![uncloaked_state(), unreadable]);
    let part = cloak_part(&mut ops, true);
    assert!(part.mutation_attempted());
    assert!(!part.tab_obligation);
    assert!(part.delete_tab.is_none());
    assert!(
        !ops.calls.iter().any(|c| c == "write:delete-tab"),
        "no pairing on unreadable readback: {:?}",
        ops.calls
    );
    assert!(!part.errors().is_empty());
}

#[test]
fn noncloaking_cloak_skips_pairing() {
    let mut ops = FakeOps::scripted(vec![uncloaked_state(), uncloaked_state()]);
    let part = cloak_part(&mut ops, true);
    assert!(part.mutation_attempted());
    assert!(!part.tab_obligation);
    assert!(part.delete_tab.is_none());
    assert_eq!(
        part.errors(),
        vec!["cloak readback mask still none".to_owned()]
    );
}

#[test]
fn log_failure_stops_new_hide_mutations_but_still_restores() {
    let mut ops = FakeOps::scripted(vec![
        uncloaked_state(),
        cloaked_state(),
        cloaked_state(),
        uncloaked_state(),
    ]);
    ops.log_ok = false;
    let part = cloak_part(&mut ops, true);
    assert!(part.mutation_attempted());
    // The unpersisted obligation must not be acted on.
    assert!(!part.tab_obligation);
    assert!(part.delete_tab.is_none());
    assert!(
        !ops.calls.iter().any(|c| c == "write:delete-tab"),
        "no pairing when the mark cannot persist: {:?}",
        ops.calls
    );
    // Restoration is never suppressed by log health.
    let restore = restore_part(&mut ops, part.tab_obligation);
    assert!(
        ops.calls.iter().any(|c| c == "write:uncloak"),
        "restore still runs: {:?}",
        ops.calls
    );
    assert!(
        !ops.calls.iter().any(|c| c == "write:add-tab"),
        "no AddTab without an owed obligation"
    );
    assert!(restore.errors().is_empty());
}

#[test]
fn expired_target_means_no_restore_writes() {
    let mut ops = FakeOps::scripted(vec![uncloaked_state(), cloaked_state()]);
    ops.fail_verify_from = Some(2);
    let part = cloak_part(&mut ops, true);
    assert!(part.mutation_attempted());
    let restore = restore_part(&mut ops, part.tab_obligation);
    assert!(
        !ops.calls.iter().any(|c| c == "write:uncloak"),
        "no uncloak write on lost identity: {:?}",
        ops.calls
    );
    assert!(
        !ops.calls.iter().any(|c| c == "write:add-tab"),
        "no add-tab write on lost identity"
    );
    assert!(!restore.errors().is_empty());
}

#[test]
fn error_after_cloak_triggers_restore() {
    let mut ops = FakeOps::scripted(vec![uncloaked_state(), cloaked_state(), uncloaked_state()]);
    ops.write_fail
        .insert("cloak", "SetCloak failed code=-1".to_owned());
    let part = cloak_part(&mut ops, false);
    assert!(part.mutation_attempted(), "failed call may have mutated");
    let restore = restore_part(&mut ops, part.tab_obligation);
    assert!(
        ops.calls.iter().any(|c| c == "write:uncloak"),
        "restore attempted after cloak error: {:?}",
        ops.calls
    );
    assert!(restore.errors().is_empty());
    assert_eq!(part.errors().len(), 1);
}

#[test]
fn identity_lost_before_write_means_no_mutation() {
    let mut ops = FakeOps::new(uncloaked_state());
    ops.fail_verify_from = Some(0);
    let part = cloak_part(&mut ops, true);
    assert!(!part.mutation_attempted());
    assert!(
        !ops.calls.iter().any(|c| c.starts_with("write:")),
        "no writes without identity: {:?}",
        ops.calls
    );
    assert!(!part.errors().is_empty());
}

#[test]
fn uncloak_always_calls_private_com_on_uncloaked_baseline() {
    // Regression: the old no-op bypass skipped SetCloak entirely on an
    // uncloaked baseline, proving nothing. Every verify reads mask 0.
    let mut ops = FakeOps::new(uncloaked_state());
    let restore = restore_part(&mut ops, false);
    let uncloak_writes = ops.calls.iter().filter(|c| *c == "write:uncloak").count();
    assert_eq!(uncloak_writes, 1, "private call runs: {:?}", ops.calls);
    assert!(restore.errors().is_empty());
    assert!(restore.uncloak.after.is_some());
}

#[test]
fn uncloak_failure_does_not_skip_owed_add_tab() {
    let mut ops = FakeOps::new(uncloaked_state());
    ops.write_fail
        .insert("uncloak", "SetCloak failed code=-1".to_owned());
    let restore = restore_part(&mut ops, true);
    assert!(
        ops.calls.iter().any(|c| c == "write:add-tab"),
        "owed AddTab attempted despite uncloak error"
    );
    assert_eq!(restore.errors().len(), 1);
    assert!(restore.errors()[0].contains("uncloak"));
}

#[test]
fn tab_obligation_is_marked_before_delete_tab_is_attempted() {
    let mut ops = FakeOps::scripted(vec![
        uncloaked_state(),
        cloaked_state(),
        cloaked_state(),
        cloaked_state(),
    ]);
    let part = cloak_part(&mut ops, true);
    assert!(part.tab_obligation);
    let obligation_at = ops
        .calls
        .iter()
        .position(|c| c == "log:obligation:add-tab")
        .expect("obligation logged");
    let delete_at = ops
        .calls
        .iter()
        .position(|c| c == "write:delete-tab")
        .expect("delete attempted");
    assert!(
        obligation_at < delete_at,
        "obligation persists before the attempt: {:?}",
        ops.calls
    );
}

#[test]
fn verified_write_verifies_before_and_reads_back_after() {
    let mut ops = FakeOps::scripted(vec![uncloaked_state(), cloaked_state()]);
    let attempt = verified_write(&mut ops, WriteOp::Cloak);
    assert!(attempt.wrote);
    assert!(attempt.error.is_none());
    assert!(attempt.log_ok);
    assert_eq!(attempt.before, Some(uncloaked_state()));
    assert_eq!(attempt.after, Some(cloaked_state()));
    let kinds = ops.log_kinds();
    assert_eq!(kinds, vec!["step:cloak"]);
    // Identity loss before the write: no write, logged STEP.
    let mut dead = FakeOps::new(uncloaked_state());
    dead.fail_verify_from = Some(0);
    let attempt = verified_write(&mut dead, WriteOp::Cloak);
    assert!(!attempt.wrote);
    assert!(attempt.error.is_some());
    assert!(!dead.calls.iter().any(|c| c.starts_with("write:")));
}
