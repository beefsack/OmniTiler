//! Scoped close-reflow integration tests (offline, portable).
//!
//! Live 3-app partial close (one window closed, its app alive, no native
//! focus move afterwards) wedged the domain pre-fix: the removed window
//! held session focus with a one-leaf focus-stack history, convergence
//! fell back to focus none, and every later report refused as
//! focus-mismatch — scope held Unreadable with zero frame authority and
//! no recovery. Closing the last-admitted window kept working, so the
//! failure depended on admission order.
//!
//! Fakes drive the runtime through the native surface with the live leg's
//! shape: sequential admits with the closer focused, worker applies,
//! readback of the plan as truth, the closer reporting empty while alive,
//! then staggered stale survivor re-reports. No live AX, no taps, no
//! Spaces, no desktop effects.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use tiler_macos::ffi::{DisplayRect, NsDisplay, SlsDisplayRow};
use tiler_macos::runtime::{
    AxCompletion, FocusCommand, FrameCommand, ObservedWindow, Runtime, RuntimeConfig,
    RuntimeHandle, ScopedObservation, ScopedWindowMeta, ShutdownOutcome, SubmitOutcome,
    TopologyMode, WindowSizeHints,
};
use tiler_macos::runtime_backend::RawKeyEvent;
use tiler_macos::runtime_backend::{AxBackend, AxBackendFactory, InputSource, InputSourceFactory};
use tiler_macos::topology;

const APP_A: u32 = 1001;
const APP_B: u32 = 1002;
const APP_C: u32 = 1003;
const FLUSH: Duration = Duration::from_secs(5);
const WAIT: Duration = Duration::from_secs(5);

// -- Fakes -------------------------------------------------------------------

/// One applied frame: pid, cmd id, window id, rect, topology revision.
type AppliedRecord = (u32, u64, String, (i32, i32, i32, i32), u64);

struct FakeInner {
    applied_tx: mpsc::Sender<AppliedRecord>,
    focus_tx: mpsc::Sender<FocusCommand>,
}

struct FakeFactory {
    inner: Arc<FakeInner>,
}

impl AxBackendFactory for FakeFactory {
    fn create(&self, _pid: u32) -> Result<Box<dyn AxBackend>, String> {
        Ok(Box::new(FakeBackend {
            inner: Arc::clone(&self.inner),
            bound: None,
        }))
    }
}

struct FakeBackend {
    inner: Arc<FakeInner>,
    bound: Option<u64>,
}

impl AxBackend for FakeBackend {
    fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion {
        let _ = self.inner.applied_tx.send((
            cmd.pid,
            cmd.cmd_id,
            cmd.window.clone(),
            (cmd.x, cmd.y, cmd.w, cmd.h),
            cmd.topology_revision,
        ));
        AxCompletion::Applied { elapsed_ms: 0 }
    }

    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
        Err("close-reflow tests drive scoped observations directly".to_owned())
    }

    fn snapshot_scoped(&mut self) -> Result<tiler_macos::runtime::ScopedBackendSnapshot, String> {
        Err("close-reflow tests drive scoped observations directly".to_owned())
    }

    fn poll_scoped(
        &mut self,
        _stop: &AtomicBool,
    ) -> Option<Result<tiler_macos::runtime::ScopedBackendSnapshot, String>> {
        // No autonomous polling: every observation is test-driven, so
        // arrivals are exactly the live leg's script. Returning `None`
        // immediately never spins: the worker parks in the frame-slot wait
        // (bounded observe slice) when no frames are queued, which also
        // bounds stop latency without any fixed delay.
        None
    }

    fn dispatch_focus(&mut self, cmd: &FocusCommand) -> AxCompletion {
        let _ = self.inner.focus_tx.send(cmd.clone());
        AxCompletion::Applied { elapsed_ms: 1 }
    }

    fn bind_effect_hook(&mut self, hook: &tiler_macos::runtime_backend::EffectHook) {
        self.bound = hook.cell.load().map(|ctx| ctx.revision);
        let _ = self.bound;
    }
}

struct FakeInput {
    rx: Mutex<Option<mpsc::Receiver<RawKeyEvent>>>,
}

impl InputSourceFactory for FakeInput {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        struct Sink(mpsc::Receiver<RawKeyEvent>);
        impl InputSource for Sink {
            fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
                loop {
                    match self.0.recv_timeout(Duration::from_millis(10)) {
                        Ok(event) => return Some(event),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if stop.load(Ordering::SeqCst) {
                                return None;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return None,
                    }
                }
            }
        }
        let rx = self.rx.lock().expect("mutex").take().expect("taken");
        Ok(Box::new(Sink(rx)))
    }

    fn describe(&self) -> &'static str {
        "fake"
    }
}

struct Fixture {
    handle: RuntimeHandle,
    applied_rx: mpsc::Receiver<AppliedRecord>,
    #[allow(dead_code)]
    input_tx: mpsc::Sender<RawKeyEvent>,
}

fn start() -> Fixture {
    let (applied_tx, applied_rx) = mpsc::channel();
    let (focus_tx, _focus_rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel();
    let inner = Arc::new(FakeInner {
        applied_tx,
        focus_tx,
    });
    let handle = Runtime::start(
        RuntimeConfig {
            topology_mode: TopologyMode::Native,
            ..RuntimeConfig::default()
        },
        Arc::new(FakeFactory {
            inner: Arc::clone(&inner),
        }),
        Arc::new(FakeInput {
            rx: Mutex::new(Some(input_rx)),
        }),
    )
    .expect("runtime starts");
    Fixture {
        handle,
        applied_rx,
        input_tx,
    }
}

// -- Helpers -----------------------------------------------------------------

fn ns(cg_id: u32, uuid: &str, x: f64, y: f64, w: f64, h: f64) -> NsDisplay {
    NsDisplay {
        cg_id,
        uuid: Some(uuid.to_owned()),
        frame: DisplayRect { x, y, w, h },
        visible: DisplayRect { x, y, w, h },
        scale: 2.0,
    }
}

fn row(id: Option<&str>, spaces: &[u64], current: Option<u64>) -> SlsDisplayRow {
    SlsDisplayRow {
        identifier: id.map(str::to_owned),
        spaces: spaces.to_vec(),
        current,
    }
}

fn user_space(_: u64) -> Option<tiler_macos::ffi::SlsSpaceType> {
    Some(tiler_macos::ffi::SlsSpaceType::User)
}

/// Single live-shaped display: one 2240x1160 output, one current Space.
fn one_display_snapshot() -> topology::TopologySnapshot {
    topology::build_snapshot(
        &[ns(1, "uuid-1", 0.0, 0.0, 2240.0, 1160.0)],
        &[row(Some("uuid-1"), &[1], Some(1))],
        Some(true),
    )
}

/// Degenerate 100x10 work area: the outer-gap inset exhausts it, so the
/// sweep holds the scope instead of projecting.
fn degenerate_snapshot() -> topology::TopologySnapshot {
    topology::build_snapshot(
        &[ns(1, "uuid-1", 0.0, 0.0, 100.0, 10.0)],
        &[row(Some("uuid-1"), &[1], Some(1))],
        Some(true),
    )
}

fn win_at(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
    ObservedWindow {
        wid,
        x,
        y,
        w,
        h,
        floating: false,
        fullscreen: false,
        hints: WindowSizeHints::none(),
    }
}

fn meta(wid: u32, spaces: &[u64]) -> ScopedWindowMeta {
    ScopedWindowMeta {
        wid,
        spaces: spaces.to_vec(),
        fullscreen: false,
        minimized: false,
        hidden: false,
    }
}

fn scoped_obs(
    pid: u32,
    generation: u64,
    seq: u64,
    members: Vec<(ObservedWindow, Vec<u64>)>,
    focused_wid: Option<u32>,
    revision: u64,
) -> ScopedObservation {
    let windows: Vec<ObservedWindow> = members.iter().map(|(window, _)| *window).collect();
    let meta = members
        .iter()
        .map(|(window, spaces)| meta(window.wid, spaces))
        .collect();
    // Agreeing global evidence: frontmost + AX + CG all name the reporter.
    let (frontmost_pid, ax_focus) = match focused_wid {
        Some(wid) => (Some(pid), Some((pid, Some(wid)))),
        None => (None, None),
    };
    // Fake attests a fresh complete CG read for its own windows.
    let mut cg_owner_wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
    cg_owner_wids.sort_unstable();
    cg_owner_wids.dedup();
    ScopedObservation {
        pid,
        generation,
        seq,
        windows,
        focused_wid,
        revision,
        frontmost_pid,
        ax_focus,
        cg_owned: focused_wid.is_some(),
        meta,
        cg_complete: true,
        cg_owner_wids,
    }
}

fn settle_watermark(handle: &RuntimeHandle) -> Vec<u64> {
    // Every counter a quiescent settle round must leave untouched.
    // `engine_jobs_processed` is deliberately excluded: the fence's own
    // barrier jobs move it. Logger drops and failure evidence ride their
    // own assertions.
    let diagnostics = handle.diagnostics();
    vec![
        diagnostics.frames_emitted,
        diagnostics.frames_coalesced,
        diagnostics.frames_redundant_skipped,
        diagnostics.frames_overlay_skipped,
        diagnostics.observations_published,
        diagnostics.observation_failures,
        diagnostics.core_replies.values().sum(),
        diagnostics.completions.values().sum(),
        diagnostics.stale_dropped,
        diagnostics.terminated_invalidated,
        diagnostics.data_dropped_full,
        diagnostics.result_dropped_full,
        diagnostics.input_forwarded,
        diagnostics.input_ignored,
        diagnostics.input_refused_no_focus,
        diagnostics.input_refused_unsupported,
        diagnostics.input_select_enqueued,
        diagnostics.input_select_refused,
        diagnostics.focus_dispatched,
        diagnostics.focus_dropped_full,
    ]
}

/// Settlement fence over every fixture app: engine barrier plus one
/// per-app worker drain-mark round each, then repeat full rounds ONLY
/// while the causal watermark above moved (a later app's engine tail must
/// never silently leave an earlier app's work outstanding when the
/// negative oracle concerns all apps). Every fence and every flush MUST
/// succeed: a `false` fence, a failed flush, or a deadline expiry is a
/// genuine failure, never an ignored timeout. Repeats park on worker
/// drain-mark events and engine barriers — never sleeps, never polls.
fn settle_known(handle: &RuntimeHandle, deadline: std::time::Instant) {
    let mut watermark = settle_watermark(handle);
    loop {
        for pid in [APP_A, APP_B, APP_C] {
            if let Some(generation) = handle.app_generation(pid) {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "settle deadline elapsed before fencing app {pid}"
                );
                assert!(
                    handle.wait_app_settled_for_test(pid, generation, remaining),
                    "app {pid} generation {generation} never settled"
                );
            }
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "settle deadline elapsed before engine flush"
        );
        assert!(handle.flush(remaining), "engine flush never completed");
        let next = settle_watermark(handle);
        if next == watermark {
            return;
        }
        watermark = next;
        assert!(
            std::time::Instant::now() < deadline,
            "system never quiesced: {watermark:?}"
        );
    }
}

fn wait_desired(handle: &RuntimeHandle, want: &BTreeSet<String>, what: &str) {
    // Parks on the shared condition until the engine's desired-plan commit
    // notifies — the commit and the reply that caused it share one
    // critical section, so no fence-then-check race. The bound is genuine.
    handle
        .wait_desired_for_test(WAIT, |frames| {
            frames
                .iter()
                .map(|frame| frame.window.clone())
                .collect::<BTreeSet<_>>()
                == *want
        })
        .unwrap_or_else(|| {
            panic!(
                "{what}: desired never converged to {want:?}: got {:?}",
                handle.last_desired_geometry()
            )
        });
}

fn drain_applied(fixture: &Fixture) {
    while fixture.applied_rx.try_recv().is_ok() {}
}

/// Wait until every wanted window has an applied record with exactly its
/// expected rect (deadline-bounded). Every received record is either the
/// awaited rect for its window or a counted skip (stale rects, already
/// settled windows) — unlike sequential single-window waits, no record
/// is silently discarded while waiting for another window.
fn wait_applied_all(fixture: &Fixture, wants: &BTreeMap<String, (i32, i32, i32, i32)>, what: &str) {
    let deadline = std::time::Instant::now() + WAIT;
    let mut pending = wants.clone();
    let mut skipped = 0u32;
    while !pending.is_empty() {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "{what}: missing applies for {pending:?} (skipped {skipped} stale)"
        );
        match fixture.applied_rx.recv_timeout(remaining) {
            Ok(record) => {
                if pending.get(&record.2) == Some(&record.3) {
                    pending.remove(&record.2);
                } else {
                    skipped += 1;
                }
            }
            Err(_) => panic!("{what}: missing applies for {pending:?} (skipped {skipped} stale)"),
        }
    }
}

/// Held-scope ring entries after `seq` (empty when no scope holds).
fn scope_holds_since(handle: &RuntimeHandle, seq: u64) -> Vec<String> {
    handle
        .log_snapshot()
        .iter()
        .filter(|entry| entry.seq > seq && entry.kind == "scope")
        .map(|entry| entry.outcome.to_owned())
        .collect()
}

fn wait_scope_hold(handle: &RuntimeHandle, outcome: &str) {
    // Parks on the shared condition until the ring's scope entry lands:
    // ring entries commit under the same lock that notifies, so the
    // awaited entry is visible without any fence. The bound is genuine.
    handle
        .wait_log_for_test(WAIT, |log| {
            log.iter()
                .any(|entry| entry.kind == "scope" && entry.outcome == outcome)
        })
        .unwrap_or_else(|| {
            panic!(
                "ring never named the scope hold {outcome}: {:?}",
                handle.log_snapshot()
            )
        });
}

fn shutdown_clean(fixture: &Fixture) {
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(
        report.outcome,
        ShutdownOutcome::Clean,
        "expected clean shutdown, got {report:?}"
    );
}

// -- Cases -------------------------------------------------------------------

/// Live 3-app partial-close shape end to end: `pids` admit in
/// `admit_order` (indices) with the closer focused like the live
/// frontmost, workers apply the plan and re-observe it (readback), then
/// the closer reports empty while alive. Survivors must reflow with
/// actuated frames and keep converging on staggered stale re-reports —
/// never wedge the scope with zero frames.
fn run_native_close_case(pids: &[u32], closer_idx: usize, admit_order: &[usize], label: &str) {
    let fixture = start();
    let revision = fixture
        .handle
        .update_topology_with(one_display_snapshot(), &user_space);
    let mut generations = BTreeMap::new();
    for pid in pids {
        generations.insert(*pid, fixture.handle.launch_app(*pid).expect("launch"));
    }
    let wid_of = |i: usize| 10 + i as u32;
    // Sequential admits; the closer is the observed focus throughout.
    for i in admit_order {
        let pid = pids[*i];
        let wid = wid_of(*i);
        let focused = if *i == closer_idx { Some(wid) } else { None };
        assert_eq!(
            fixture.handle.submit_scoped_observation(scoped_obs(
                pid,
                generations[&pid],
                1,
                vec![(win_at(wid, 0, 0, 100, 100), vec![1])],
                focused,
                revision
            )),
            SubmitOutcome::Accepted,
            "{label}: admit must submit"
        );
    }
    assert!(fixture.handle.flush(FLUSH));
    let want: BTreeSet<String> = pids
        .iter()
        .enumerate()
        .map(|(i, pid)| format!("{pid}_{}", wid_of(i)))
        .collect();
    wait_desired(&fixture.handle, &want, "admit");
    // Live readback: report the planned geometry back as observed truth so
    // retained truth equals the plan before the close, as on the desktop.
    for (i, pid) in pids.iter().enumerate() {
        let wid = wid_of(i);
        let frame = fixture
            .handle
            .last_desired_geometry()
            .iter()
            .find(|frame| frame.window == format!("{pid}_{wid}"))
            .cloned()
            .expect("admitted frame");
        let focused = if i == closer_idx { Some(wid) } else { None };
        assert_eq!(
            fixture.handle.submit_scoped_observation(scoped_obs(
                *pid,
                generations[pid],
                2,
                vec![(win_at(wid, frame.x, frame.y, frame.w, frame.h), vec![1])],
                focused,
                revision
            )),
            SubmitOutcome::Accepted,
            "{label}: readback must submit"
        );
    }
    assert!(fixture.handle.flush(FLUSH));
    wait_desired(&fixture.handle, &want, "readback");
    // Let applies land, then drain so post-close applies are attributable.
    // The settlement fence proves every worker pass drained through the
    // causal marks, so the single drain below is exact — no fixed window.
    settle_known(&fixture.handle, std::time::Instant::now() + WAIT);
    drain_applied(&fixture);
    let pre_close: BTreeMap<String, (i32, i32, i32, i32)> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|frame| (frame.window.clone(), (frame.x, frame.y, frame.w, frame.h)))
        .collect();
    let accepted_before = fixture
        .handle
        .diagnostics()
        .core_replies
        .get("reconcile-accepted")
        .copied()
        .unwrap_or(0);
    let log_seq = fixture
        .handle
        .log_snapshot()
        .iter()
        .map(|entry| entry.seq)
        .max()
        .unwrap_or(0);

    // The close: empty observation from the live closer app.
    let closer = pids[closer_idx];
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            closer,
            generations[&closer],
            3,
            Vec::new(),
            None,
            revision
        )),
        SubmitOutcome::Accepted,
        "{label}: close must submit"
    );
    assert!(fixture.handle.flush(FLUSH));
    // Survivors reflow: the desired plan holds exactly the survivors, and
    // every survivor actuates to its new frame.
    let survivors: BTreeSet<String> = want
        .iter()
        .filter(|window| !window.starts_with(&format!("{closer}_")))
        .cloned()
        .collect();
    wait_desired(&fixture.handle, &survivors, "close reflow");
    let want_rects: BTreeMap<String, (i32, i32, i32, i32)> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|frame| (frame.window.clone(), (frame.x, frame.y, frame.w, frame.h)))
        .collect();
    wait_applied_all(&fixture, &want_rects, "close reflow actuation");
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0)
            > accepted_before,
        "{label}: close must apply: {:?}",
        diagnostics.core_replies
    );
    assert!(
        scope_holds_since(&fixture.handle, log_seq).is_empty(),
        "{label}: close must not hold any scope: {:?}",
        scope_holds_since(&fixture.handle, log_seq)
    );
    // Live stagger: survivors re-report stale pre-close frames while still
    // unfocused. Convergence must hold (no wedge, no scope hold).
    drain_applied(&fixture);
    for (i, pid) in pids.iter().enumerate() {
        if *pid == closer {
            continue;
        }
        let wid = wid_of(i);
        let (x, y, w, h) = pre_close[&format!("{pid}_{wid}")];
        assert_eq!(
            fixture.handle.submit_scoped_observation(scoped_obs(
                *pid,
                generations[pid],
                4,
                vec![(win_at(wid, x, y, w, h), vec![1])],
                None,
                revision
            )),
            SubmitOutcome::Accepted,
            "{label}: survivor re-report must submit"
        );
    }
    assert!(fixture.handle.flush(FLUSH));
    wait_desired(&fixture.handle, &survivors, "staggered reconverge");
    assert!(
        scope_holds_since(&fixture.handle, log_seq).is_empty(),
        "{label}: staggered reports must not hold any scope: {:?}",
        scope_holds_since(&fixture.handle, log_seq)
    );
    shutdown_clean(&fixture);
}

#[test]
fn native_close_focused_first_admitted_reflows_survivors() {
    // Exact live LD shape: the closer admits first and holds focus, then
    // closes its window while alive. Pre-fix this wedged the scope with
    // zero frames and no recovery.
    run_native_close_case(&[APP_A, APP_B, APP_C], 0, &[0, 1, 2], "close-first");
}

#[test]
fn native_close_middle_and_last_admitted_reflow_survivors() {
    // Every other close position reflows too (fixed admit order, the
    // closer focused in each leg).
    run_native_close_case(&[APP_A, APP_B, APP_C], 1, &[0, 1, 2], "close-middle");
    run_native_close_case(&[APP_A, APP_B, APP_C], 2, &[0, 1, 2], "close-last");
}

#[test]
fn native_two_app_close_control_reflows_survivor() {
    // Two-app control from the live LD2 leg: the focused closer leaves a
    // single survivor, which expands with an actuated frame.
    run_native_close_case(&[APP_A, APP_B], 1, &[0, 1], "two-app");
}

#[test]
fn held_scope_names_its_reason_in_the_ring() {
    // A geometrically exhausted work area holds the scope instead of
    // actuating: the applied reply still counts as accepted with zero
    // frames, but the ring names the hold reason — previously a held
    // scope was silent (accepted with count 0 and no reason anywhere).
    let fixture = start();
    let revision = fixture
        .handle
        .update_topology_with(degenerate_snapshot(), &user_space);
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            generation,
            1,
            vec![(win_at(7, 0, 0, 50, 5), vec![1])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    wait_scope_hold(&fixture.handle, "held-constraint-clamp");
    assert!(
        fixture.handle.last_desired_geometry().is_empty(),
        "a held scope plans no geometry"
    );
    assert_eq!(
        fixture
            .handle
            .diagnostics()
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        1,
        "the held reply still counts as accepted: {:?}",
        fixture.handle.diagnostics().core_replies
    );
    shutdown_clean(&fixture);
}
