//! Event-driven owner-wait regressions: deadline selection, directory-watch
//! rearm ordering, ledger name-only vs settings full-mask scoping,
//! extra-handle wakes, atomic request publishing, and async completion
//! signals.
//!
//! All hermetic: own temp directories and handles only, no live tiler run,
//! no desktop mutation (icon lookups use HWND 0, which refuses before any
//! native call).

use super::dir_watch::DirWatch;
use super::observe::pump_wait_with_intent;
use super::run_loop_wait::{OwnerDeadlines, owner_timeout_ms};
use super::{OwnerWaitCounts, WaitWake, pump_waits_line};
use std::time::{Duration, Instant};

fn idle_deadlines() -> OwnerDeadlines {
    OwnerDeadlines {
        restore_until: None,
        pending_until: None,
        pager_maintenance_at: None,
        snap_retry_at: None,
        pager_retry_at: None,
        pager_icon_retry_at: None,
        gaming_due_at: None,
        monitor_due_at: None,
        run_end: None,
        gesture_open: false,
        degraded_poll: false,
    }
}

#[test]
fn healthy_idle_selects_infinite() {
    // No armed retry, no restore, unbounded run, no gesture:
    // the wait blocks (Win32 INFINITE), replacing the old 100ms poll.
    let now = Instant::now();
    assert_eq!(owner_timeout_ms(now, &idle_deadlines()), u32::MAX);
}

#[test]
fn each_armed_deadline_bounds_the_wait() {
    // Every retained period arms the wait with its own specific reason;
    // nothing falls back to an arbitrary poll.
    let now = Instant::now();
    let mut deadlines = idle_deadlines();
    deadlines.restore_until = Some(now + Duration::from_millis(2000));
    let timeout = owner_timeout_ms(now, &deadlines);
    assert!((1900..=2000).contains(&timeout), "{timeout}");
    // Absent hide-ack bound: the pending gesture finish wakes its service
    // turn at the deadline instead of stranding (the timeout path still
    // refuses with no drop; see the pending-drop timeout test).
    let mut deadlines = idle_deadlines();
    deadlines.pending_until = Some(now + Duration::from_millis(500));
    let timeout = owner_timeout_ms(now, &deadlines);
    assert!((400..=500).contains(&timeout), "{timeout}");
    // Visible-pager maintenance bound: creation retry and topmost
    // recovery share one deadline while strips show, without polling
    // while hidden.
    let mut deadlines = idle_deadlines();
    deadlines.pager_maintenance_at = Some(now + Duration::from_secs(2));
    assert!((1900..=2000).contains(&owner_timeout_ms(now, &deadlines)));
    let mut deadlines = idle_deadlines();
    deadlines.snap_retry_at = Some(now + Duration::from_secs(5));
    assert!((4900..=5000).contains(&owner_timeout_ms(now, &deadlines)));
    let mut deadlines = idle_deadlines();
    deadlines.pager_retry_at = Some(now + Duration::from_secs(5));
    assert!((4900..=5000).contains(&owner_timeout_ms(now, &deadlines)));
    let mut deadlines = idle_deadlines();
    deadlines.pager_icon_retry_at = Some(now + Duration::from_secs(5));
    assert!((4900..=5000).contains(&owner_timeout_ms(now, &deadlines)));
    let mut deadlines = idle_deadlines();
    deadlines.gaming_due_at = Some(now + Duration::from_secs(300));
    let timeout = owner_timeout_ms(now, &deadlines);
    assert!(timeout > 299_000 && timeout <= 300_000, "{timeout}");
    let mut deadlines = idle_deadlines();
    deadlines.monitor_due_at = Some(now + Duration::from_secs(2));
    assert!((1900..=2000).contains(&owner_timeout_ms(now, &deadlines)));
    let mut deadlines = idle_deadlines();
    deadlines.run_end = Some(now + Duration::from_secs(7));
    assert!((6900..=7000).contains(&owner_timeout_ms(now, &deadlines)));
}

#[test]
fn deadline_selects_the_minimum_and_caps_degraded_poll() {
    // Minimum wins across armed inputs; the degraded watch fallback caps
    // (never extends) the wait.
    let now = Instant::now();
    let mut deadlines = idle_deadlines();
    deadlines.restore_until = Some(now + Duration::from_millis(2000));
    deadlines.run_end = Some(now + Duration::from_millis(500));
    deadlines.gesture_open = true;
    let timeout = owner_timeout_ms(now, &deadlines);
    assert!((90..=100).contains(&timeout), "{timeout}");
    // The pending-finish bound wins over later armed inputs.
    let mut deadlines = idle_deadlines();
    deadlines.pending_until = Some(now + Duration::from_millis(250));
    deadlines.run_end = Some(now + Duration::from_secs(5));
    deadlines.pager_maintenance_at = Some(now + Duration::from_secs(2));
    let timeout = owner_timeout_ms(now, &deadlines);
    assert!((150..=250).contains(&timeout), "{timeout}");
    // Past-due deadlines expire immediately (zero), never negative wrap.
    let mut deadlines = idle_deadlines();
    deadlines.restore_until = Some(now - Duration::from_secs(1));
    assert_eq!(owner_timeout_ms(now, &deadlines), 0);
    // Degraded poll bounds an otherwise infinite wait...
    let mut deadlines = idle_deadlines();
    deadlines.degraded_poll = true;
    assert_eq!(
        owner_timeout_ms(now, &deadlines),
        super::run_loop_wait::DEGRADED_POLL_MS as u32
    );
    // ...and caps (never extends) an earlier armed deadline.
    let mut deadlines = idle_deadlines();
    deadlines.restore_until = Some(now + Duration::from_millis(2000));
    deadlines.degraded_poll = true;
    assert_eq!(
        owner_timeout_ms(now, &deadlines),
        super::run_loop_wait::DEGRADED_POLL_MS as u32
    );
}

fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tiler-owner-wait-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_file(dir: &std::path::Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).expect("write temp file");
}

/// Append-only write mirroring `log_json_at` (open-append-write per line).
fn append_file(dir: &std::path::Path, name: &str, body: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(name))
        .expect("open temp file for append");
    writeln!(file, "{body}").expect("append temp file");
}

/// Settle a watch to quiet: repeated rearm plus a short sleep absorbs the
/// trailing change records one file write can generate (create, size,
/// last-write arrive separately). Returns true once the handle reads
/// clear. A genuinely stuck (always-signaled) handle never settles here.
fn settle_watch(watch: &DirWatch) -> bool {
    for _ in 0..200 {
        if !watch.rearm() {
            return false;
        }
        if !watch.pending() {
            // One more beat: a record landing right after the probe must
            // re-signal instead of hiding.
            std::thread::sleep(Duration::from_millis(10));
            if !watch.pending() {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn dir_write_signals_and_settles_to_quiet() {
    // Basic contract: a file create signals the name-only ledger watch;
    // after the turn's rearm the handle settles back to quiet (no spin),
    // ready for the next change.
    let dir = unique_temp_dir("signal");
    let watch = DirWatch::watched_names(&dir).expect("watch temp dir");
    assert!(!watch.pending());
    write_file(&dir, "stop.request", "hello");
    assert!(watch.pending(), "write signals the watch");
    assert!(settle_watch(&watch), "watch settles to quiet after rearm");
    assert!(!watch.pending(), "no spin once settled");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn rearm_before_inspect_keeps_concurrent_writes() {
    // The race-safe ordering: rearm first, then inspect; a write landing
    // between the rearm and the inspection re-signals and is observed by
    // the next wait instead of being lost. (The losing order, inspect
    // then rearm, would drop exactly this write.)
    let dir = unique_temp_dir("race");
    let watch = DirWatch::watched(&dir).expect("watch temp dir");
    write_file(&dir, "workspace.request", "first");
    assert!(watch.pending());
    // Turn processing: rearm BEFORE inspection.
    assert!(watch.rearm());
    // Concurrent write lands between the rearm and the inspection.
    write_file(&dir, "workspace.request", "second");
    // Inspection still sees the latest content...
    let seen = std::fs::read_to_string(dir.join("workspace.request")).expect("read");
    assert_eq!(seen, "second");
    // ...and the concurrent write re-signaled, so the next wait returns
    // immediately instead of losing it.
    assert!(watch.pending(), "concurrent write survives the rearm");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn burst_coalesces_and_own_delete_recovers_without_spin() {
    // A burst of writes coalesces to pending signals (consume-once
    // inspection still sees the latest), and the owner's own consume
    // delete re-signals, then settles back to quiet after its spare
    // turns: delayed records, never a spin. Runs under the name-only
    // ledger mask: the burst's creates plus the consume delete are name
    // events, so coalescing holds where the owner actually watches.
    let dir = unique_temp_dir("burst");
    let watch = DirWatch::watched_names(&dir).expect("watch temp dir");
    for index in 0..8 {
        write_file(&dir, "settings.json", &format!("{{\"revision\":{index}}}"));
    }
    assert!(watch.pending(), "burst signals");
    assert!(watch.rearm());
    let seen = std::fs::read_to_string(dir.join("settings.json")).expect("read");
    assert!(seen.contains("\"revision\":7"), "latest wins, {seen}");
    // Own consume-before-dispatch delete: signals, then settles quiet.
    std::fs::remove_file(dir.join("settings.json")).expect("own delete");
    assert!(watch.pending(), "own delete signals");
    assert!(
        settle_watch(&watch),
        "watch settles quiet after the spare turns"
    );
    assert!(!watch.pending(), "no spin once settled");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_directory_reports_none_for_degraded_poll() {
    // No wedge, no spin: an unwatched directory reports `None` and the
    // caller degrades to the bounded poll with a reason.
    let missing = std::env::temp_dir().join(format!(
        "tiler-owner-wait-absent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    assert!(!missing.exists());
    assert!(DirWatch::watched(&missing).is_none());
    assert!(DirWatch::watched_names(&missing).is_none());
}

fn wake_signaled(handle: &std::os::windows::io::OwnedHandle) -> bool {
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
    let raw = handle.as_handle().as_raw_handle();
    unsafe {
        windows_sys::Win32::System::Threading::WaitForSingleObject(raw, 0)
            == windows_sys::Win32::Foundation::WAIT_OBJECT_0
    }
}

fn auto_event() -> std::os::windows::io::OwnedHandle {
    use std::os::windows::io::FromRawHandle as _;
    let raw = unsafe {
        windows_sys::Win32::System::Threading::CreateEventW(
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
        )
    };
    assert!(!raw.is_null());
    unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw) }
}

#[test]
fn extra_notify_handle_wakes_and_consumes_without_touching_peers() {
    // Pre-wait signal (offered before the wait) returns immediately with
    // the notify reason and a full turn; bursts coalesce to one wake and
    // the peer wakes stay pending.
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
    let intent = auto_event();
    let ack = auto_event();
    let notify = auto_event();
    unsafe {
        windows_sys::Win32::System::Threading::SetEvent(notify.as_handle().as_raw_handle());
    }
    let raw = notify.as_handle().as_raw_handle();
    let woke = pump_wait_with_intent(0, Some(&intent), Some(&ack), std::slice::from_ref(&raw));
    assert!(woke.notify, "extra handle reports notify");
    assert!(woke.full_turn(), "notify always takes a full turn");
    assert!(!woke.input && !woke.ack, "no reason confusion");
    assert!(!wake_signaled(&notify), "auto-reset consumes the notify");
    assert!(!wake_signaled(&intent), "peer intent untouched");
    assert!(!wake_signaled(&ack), "peer ack untouched");
    // Quiet extras: no notify reason (genuine timeout, ambient message,
    // or the pre-existing failed fallback, as on a quiet desktop).
    let woke = pump_wait_with_intent(0, Some(&intent), Some(&ack), std::slice::from_ref(&raw));
    assert!(!woke.notify);
    assert!(woke.timeout || woke.message || woke.failed);
}

#[test]
fn dir_watch_drives_the_extra_handle_end_to_end() {
    // The name-only ledger watch handle plugs into the wait set: a file
    // write offered before the wait returns immediately with notify (the
    // stop / request wake path).
    let dir = unique_temp_dir("waitset");
    let watch = DirWatch::watched_names(&dir).expect("watch temp dir");
    // Borrow the raw notification handle without taking ownership: the
    // watch keeps arming it while the wait only observes.
    let raw = watch.raw() as _;
    write_file(&dir, crate::lifecycle::STOP_REQUEST_FILE, "stop");
    let woke = pump_wait_with_intent(0, None, None, std::slice::from_ref(&raw));
    assert!(woke.notify, "stop-file write wakes the wait set");
    assert!(woke.full_turn());
    assert!(watch.rearm());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn icon_service_completion_handle_signals_on_result() {
    // HWND 0 refuses before any native call (hermetic, no desktop
    // effects): the finished lookup still signals completion so results
    // never strand behind an idle wait.
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
    let service = crate::pager_icons_sys::PagerIconService::try_new().expect("icon worker spawns");
    let handle = service.completion_handle().expect("completion event");
    let raw = handle.as_handle().as_raw_handle();
    let signaled = || unsafe {
        windows_sys::Win32::System::Threading::WaitForSingleObject(raw, 0)
            == windows_sys::Win32::Foundation::WAIT_OBJECT_0
    };
    assert!(!signaled(), "no spurious completion wake");
    service.request(vec![crate::workspace::WindowKey {
        hwnd: 0,
        pid: 0,
        creation: "test-none".to_owned(),
    }]);
    let start = Instant::now();
    let mut drained = 0;
    let mut saw_signal = false;
    while drained == 0 && start.elapsed() < Duration::from_secs(10) {
        if signaled() {
            saw_signal = true;
        }
        drained = service.drain().len();
        if drained == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(drained, 1, "refused lookup still completes");
    // The signal may have landed between the last probe and the drain
    // (auto-reset holds it until observed): one final probe closes that.
    saw_signal |= signaled();
    assert!(saw_signal, "completion signaled the owner wait");
}

fn wait_monitor_idle(monitor: &crate::gaming::GamingMonitor) {
    let start = Instant::now();
    while monitor.in_flight() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "worker pass did not finish"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn gaming_completion_survives_owner_teardown() {
    // The detached worker may complete after owner teardown: the hint
    // closure owns a shared event clone, so the handle stays valid until
    // the last clone drops (never a use-after-close on a reused HANDLE),
    // and detaching first keeps a delayed pass safe and quiet.
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let probe: crate::gaming::ProbeFn = Arc::new({
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        move || {
            entered.store(true, Ordering::SeqCst);
            while !release.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
            crate::gaming::GamingSnapshot::unknown()
        }
    });
    let monitor = crate::gaming::GamingMonitor::new(probe);
    let event = Arc::new(auto_event());
    let observer = Arc::clone(&event);
    let owned = Arc::clone(&event);
    monitor.set_completion_notify(Some(Arc::new(move || unsafe {
        windows_sys::Win32::System::Threading::SetEvent(owned.as_handle().as_raw_handle());
    })));
    // Owner teardown drops its clone while the pass is still blocked.
    drop(event);
    monitor.request_refresh(crate::gaming::RefreshReason::Startup);
    let start = Instant::now();
    while !entered.load(Ordering::SeqCst) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "pass never started"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    release.store(true, Ordering::SeqCst);
    wait_monitor_idle(&monitor);
    // The worker-held clone signaled a live handle: exactly the
    // use-after-teardown path that a raw captured HANDLE would break.
    let raw = observer.as_handle().as_raw_handle();
    let signaled = unsafe {
        windows_sys::Win32::System::Threading::WaitForSingleObject(raw, 0)
            == windows_sys::Win32::Foundation::WAIT_OBJECT_0
    };
    assert!(signaled, "delayed completion signals a live handle");
    assert_eq!(monitor.drain_logs().len(), 2);
    // Detach then run again: safe, quiet, still completes.
    monitor.set_completion_notify(None);
    monitor.request_refresh(crate::gaming::RefreshReason::Recheck);
    wait_monitor_idle(&monitor);
    assert_eq!(monitor.drain_logs().len(), 2);
}

#[test]
fn trace_throttle_never_arms_the_wait() {
    // Diagnostics must not wake the owner: `OwnerDeadlines` carries no
    // trace member at all (compile-enforced by construction above), so
    // even a long-overdue reporting throttle still selects INFINITE here.
    // The throttle lives only as elapsed time on naturally occurring
    // turns plus one final read-only flush at loop exit.
    let now = Instant::now();
    assert_eq!(owner_timeout_ms(now, &idle_deadlines()), u32::MAX);
    // Every other retained deadline still bounds the wait on its own, so
    // removing the trace member took nothing else with it.
    let mut deadlines = idle_deadlines();
    deadlines.run_end = Some(now + Duration::from_secs(7));
    assert!((6900..=7000).contains(&owner_timeout_ms(now, &deadlines)));
}

#[test]
fn pump_waits_line_carries_the_full_split_without_a_monitor_check() {
    // The report builder takes no monitor input at all: periodic lines
    // ride whatever natural turn is due, and the final flush needs no
    // monitor event. Owner notify/input/ack/msg/timeout/failed plus the
    // ack-only subset all arrive, alongside the existing input/render
    // snapshots; scope/excludes markers are unchanged. Counter
    // accounting is preserved: genuine timeout stays separate from
    // failed, and ack-only stays a subset of waits.
    let mut ow = OwnerWaitCounts::default();
    ow.note_wait(WaitWake {
        input: true,
        ack: true,
        message: true,
        notify: true,
        timeout: false,
        failed: false,
    });
    ow.note_ack_only();
    ow.note_wait(WaitWake {
        input: false,
        ack: false,
        message: false,
        notify: false,
        timeout: true,
        failed: false,
    });
    ow.note_wait(WaitWake {
        input: false,
        ack: false,
        message: false,
        notify: false,
        timeout: false,
        failed: true,
    });
    let line = pump_waits_line(&ow, (11, 9, 2, 3, 4, 5), (8, 1, 2, 3, 2, 6), None);
    assert_eq!(line["event"], "pump-waits");
    assert_eq!(line["owner_waits"], 3);
    assert_eq!(line["owner_ack_only"], 1);
    assert_eq!(line["owner_input"], 1);
    assert_eq!(line["owner_ack"], 1);
    assert_eq!(line["owner_msg"], 1);
    assert_eq!(line["owner_notify"], 1);
    assert_eq!(line["owner_timeout"], 1);
    assert_eq!(line["owner_failed"], 1);
    assert_eq!(line["input_turns"], 11);
    assert_eq!(line["input_waits"], 9);
    assert_eq!(line["input_work"], 2);
    assert_eq!(line["input_msg"], 3);
    assert_eq!(line["input_timeout"], 4);
    assert_eq!(line["input_failed"], 5);
    assert_eq!(line["render_waits"], 8);
    assert_eq!(line["render_work"], 1);
    assert_eq!(line["render_msg"], 2);
    assert_eq!(line["render_timeout"], 3);
    assert_eq!(line["render_failed"], 2);
    assert_eq!(line["render_acks"], 6);
    assert_eq!(line["scope"], "outer-only-plus-native-waits");
    assert_eq!(
        line["excludes"],
        "focus-proof-nested-waits,stop-short-circuit"
    );
    assert!(
        line.get("flushed").is_none(),
        "periodic lines carry no marker"
    );
    // Pure builder, no double-drain side effects: a second call over the
    // same inputs reports identically.
    assert_eq!(
        pump_waits_line(&ow, (11, 9, 2, 3, 4, 5), (8, 1, 2, 3, 2, 6), None),
        line
    );
}

#[test]
fn pump_waits_final_flush_is_observable_with_no_work() {
    // A quiet run (zero waits, zero snapshots, no monitor event ever)
    // still leaves one measurable accumulated line at clean loop exit,
    // marked `end-of-run` so it diffs honestly against periodic lines.
    let ow = OwnerWaitCounts::default();
    let line = pump_waits_line(
        &ow,
        (0, 0, 0, 0, 0, 0),
        (0, 0, 0, 0, 0, 0),
        Some("end-of-run"),
    );
    assert_eq!(line["event"], "pump-waits");
    assert_eq!(line["owner_waits"], 0);
    assert_eq!(line["owner_ack_only"], 0);
    assert_eq!(line["owner_timeout"], 0);
    assert_eq!(line["owner_failed"], 0);
    assert_eq!(line["input_turns"], 0);
    assert_eq!(line["render_acks"], 0);
    assert_eq!(line["flushed"], "end-of-run");
}

#[test]
fn ledger_name_watch_ignores_own_log_appends() {
    // The live self-wake: the owner's per-turn trace-log appends land in
    // the watched ledger dir. Under the name-only ledger mask, content
    // appends to an existing log never signal, so trace turns stop waking
    // the owner through its own log.
    let dir = unique_temp_dir("logself");
    let watch = DirWatch::watched_names(&dir).expect("watch temp dir");
    // The log's initial create is a name event (one bounded startup wake);
    // settle past it like the loop's first turn does.
    append_file(&dir, "run-test.log", "{\"event\":\"tile-start\"}");
    assert!(watch.pending(), "initial create still signals once");
    assert!(settle_watch(&watch), "settles past the create");
    // Steady-state appends: none may signal.
    for turn in 0..5 {
        append_file(
            &dir,
            "run-test.log",
            &format!("{{\"event\":\"tick\",\"tick\":{turn}}}"),
        );
        std::thread::sleep(Duration::from_millis(20));
        assert!(!watch.pending(), "own log append must not wake the owner");
    }
    // A quiet beat: a delayed record would have landed by now (the
    // positive tests observe signals immediately).
    std::thread::sleep(Duration::from_millis(100));
    assert!(!watch.pending(), "no delayed self-signal");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn settings_watch_still_catches_in_place_rewrite() {
    // Mask contrast: the settings watch keeps content flags because
    // external editors may save in place with no rename. The same
    // content-only change that the ledger mask ignores must signal here.
    let dir = unique_temp_dir("settingsself");
    let watch = DirWatch::watched(&dir).expect("watch temp dir");
    write_file(&dir, "settings.json", "{\"revision\":1}");
    assert!(watch.pending(), "create signals");
    assert!(settle_watch(&watch), "settles past the create");
    write_file(&dir, "settings.json", "{\"revision\":2}");
    assert!(
        watch.pending(),
        "in-place content change still wakes settings"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ledger_name_watch_catches_atomic_publish_and_consume_delete() {
    // Request path end to end under the ledger mask: atomic publish (temp
    // plus rename, mirroring `cmd_workspace`) wakes with the body already
    // complete and visible, and the owner's consume delete wakes at most
    // one bounded spare turn before settling quiet (no loop: inspection
    // creates no files).
    let dir = unique_temp_dir("atomicreq");
    let watch = DirWatch::watched_names(&dir).expect("watch temp dir");
    crate::storage::publish_no_overwrite(&dir, "workspace.request", b"{\"v\":1}", "test")
        .expect("atomic publish");
    assert!(watch.pending(), "atomic publish wakes the ledger watch");
    // Rearm-before-inspect: the body is complete at signal time, never
    // partial (rename atomicity), so the consume path sees it whole.
    assert!(watch.rearm());
    let seen = std::fs::read_to_string(dir.join("workspace.request")).expect("read");
    assert_eq!(seen, "{\"v\":1}", "published body complete at signal time");
    std::fs::remove_file(dir.join("workspace.request")).expect("consume delete");
    assert!(
        watch.pending(),
        "consume delete wakes at most one spare turn"
    );
    assert!(settle_watch(&watch), "settles quiet: no self-wake loop");
    assert!(!watch.pending());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn publish_replace_overwrites_without_partial() {
    // Stop-request semantics: repeated publishes replace (never refuse),
    // and readers always see one complete body (old or new, never a mix).
    let dir = unique_temp_dir("stopreplace");
    crate::storage::publish_replace(&dir, "stop.request", b"creation-1", "test")
        .expect("first stop publish");
    assert_eq!(
        std::fs::read_to_string(dir.join("stop.request")).expect("read"),
        "creation-1"
    );
    crate::storage::publish_replace(&dir, "stop.request", b"creation-2", "test")
        .expect("repeated stop replaces, never refuses");
    assert_eq!(
        std::fs::read_to_string(dir.join("stop.request")).expect("read"),
        "creation-2"
    );
    std::fs::remove_dir_all(&dir).ok();
}
