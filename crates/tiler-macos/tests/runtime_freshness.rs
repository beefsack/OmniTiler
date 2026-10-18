//! Runtime freshness/sequencing integration tests (offline, portable).
//!
//! Full-world watermark/fencing regressions extracted from
//! `runtime_phase1.rs`: redundant-write skips, empty-observation removal,
//! midflight/post-accept staleness, mark-without-job fencing, queue
//! saturation overmarking, and stale-queue classification. Deterministic
//! fakes drive the runtime through its public typed handoffs: barriers and
//! channels synchronize threads, never timing luck. No live AX, no taps, no
//! Spaces, no desktop effects.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use tiler_macos::runtime::{
    AxCompletion, AxObservation, FrameCommand, ObservedWindow, Runtime, RuntimeConfig,
    RuntimeHandle, ShutdownOutcome, SubmitOutcome,
};
use tiler_macos::runtime_backend::{
    AxBackend, AxBackendFactory, BackendSnapshot, InputSource, InputSourceFactory, RawKeyEvent,
};

// -- Fakes ---------------------------------------------------------------------

const APP_A: u32 = 1001;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

// Entered/applied records mirror `runtime_phase1.rs`; only the `window`/`rect`
// projections are read here.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct AppliedRecord {
    pid: u32,
    generation: u64,
    cmd_id: u64,
    window: String,
    rect: (i32, i32, i32, i32),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct EnteredRecord {
    pid: u32,
    cmd_id: u64,
}

struct Gate {
    released: Mutex<bool>,
    cond: Condvar,
}

struct FakeAxInner {
    applied_tx: mpsc::Sender<AppliedRecord>,
    entered_tx: mpsc::Sender<EnteredRecord>,
    gates: Mutex<BTreeMap<u32, Arc<Gate>>>,
    create_gates: Mutex<BTreeMap<u32, Arc<Gate>>>,
    /// Wake-driven observation pumps: a worker blocked here wakes
    /// immediately on send; the bound only caps stop latency.
    poll_gates: Mutex<BTreeMap<u32, Arc<Gate>>>,
    /// Explicit snapshot content per pid. Absent means "not configured":
    /// `snapshot()` fails honestly instead of publishing an empty list as
    /// truth, so manually driven tests (which never configure it) keep
    /// their own sequence space.
    snapshots: Mutex<BTreeMap<u32, Vec<ObservedWindow>>>,
    snapshot_fail: Mutex<BTreeSet<u32>>,
    /// Autonomous observation inboxes, one per pid, installed before launch
    /// and taken by the owning worker at creation.
    obs_inboxes: Mutex<BTreeMap<u32, mpsc::Receiver<Result<BackendSnapshot, String>>>>,
    panic_pids: Mutex<BTreeSet<u32>>,
    fail_create: AtomicBool,
    timeout_once: Mutex<BTreeSet<u32>>,
    /// PIDs whose next apply reports `Failed` with a fixed OS-status reason
    /// (one-shot): proves completion sanitization preserves the cause.
    fail_status_once: Mutex<BTreeSet<u32>>,
    shutdown_fail: Mutex<BTreeSet<u32>>,
}

struct FakeAxFactory {
    inner: Arc<FakeAxInner>,
}

impl AxBackendFactory for FakeAxFactory {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String> {
        if self.inner.fail_create.load(Ordering::SeqCst) {
            return Err("fake backend refused startup".to_owned());
        }
        // Creation gates (cloned out before waiting) let tests rendezvous
        // with a worker that is stuck inside backend creation.
        let gate = self
            .inner
            .create_gates
            .lock()
            .expect("mutex")
            .get(&pid)
            .cloned();
        if let Some(gate) = gate {
            let mut released = gate.released.lock().expect("mutex");
            while !*released {
                released = gate.cond.wait(released).expect("mutex");
            }
        }
        Ok(Box::new(FakeAxBackend {
            pid,
            inner: Arc::clone(&self.inner),
            obs_rx: self.inner.obs_inboxes.lock().expect("mutex").remove(&pid),
        }))
    }
}

struct FakeAxBackend {
    pid: u32,
    inner: Arc<FakeAxInner>,
    obs_rx: Option<mpsc::Receiver<Result<BackendSnapshot, String>>>,
}

impl AxBackend for FakeAxBackend {
    fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion {
        // Entered is signalled before any behaviour so tests can
        // deterministically rendezvous with a blocked or panicking apply.
        let _ = self.inner.entered_tx.send(EnteredRecord {
            pid: cmd.pid,
            cmd_id: cmd.cmd_id,
        });
        if self
            .inner
            .panic_pids
            .lock()
            .expect("mutex")
            .contains(&self.pid)
        {
            panic!("fake AX backend panic");
        }
        // Clone the gate out of the map first: holding the map guard
        // across the condvar wait would deadlock every other worker.
        let gate = self
            .inner
            .gates
            .lock()
            .expect("mutex")
            .get(&self.pid)
            .cloned();
        if let Some(gate) = gate {
            let mut released = gate.released.lock().expect("mutex");
            while !*released {
                released = gate.cond.wait(released).expect("mutex");
            }
        }
        if self
            .inner
            .timeout_once
            .lock()
            .expect("mutex")
            .remove(&self.pid)
        {
            return AxCompletion::Timeout { timeout_ms: 2000 };
        }
        if self
            .inner
            .fail_status_once
            .lock()
            .expect("mutex")
            .remove(&self.pid)
        {
            return AxCompletion::Failed {
                reason: "set: kAXErrorCannotComplete".to_owned(),
            };
        }
        let _ = self.inner.applied_tx.send(AppliedRecord {
            pid: cmd.pid,
            generation: cmd.generation,
            cmd_id: cmd.cmd_id,
            window: cmd.window.clone(),
            rect: (cmd.x, cmd.y, cmd.w, cmd.h),
        });
        AxCompletion::Applied { elapsed_ms: 0 }
    }

    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
        if self
            .inner
            .snapshot_fail
            .lock()
            .expect("mutex")
            .contains(&self.pid)
        {
            return Err(format!("fake snapshot refused for pid {}", self.pid));
        }
        self.inner
            .snapshots
            .lock()
            .expect("mutex")
            .get(&self.pid)
            .cloned()
            .ok_or_else(|| format!("fake snapshot not configured for pid {}", self.pid))
    }

    fn poll_observation(&mut self, stop: &AtomicBool) -> Option<Result<BackendSnapshot, String>> {
        // Poll gates (cloned out before waiting) let tests rendezvous with
        // a worker stuck inside its observation pump.
        let gate = self
            .inner
            .poll_gates
            .lock()
            .expect("mutex")
            .get(&self.pid)
            .cloned();
        if let Some(gate) = gate {
            let mut released = gate.released.lock().expect("mutex");
            while !*released {
                released = gate.cond.wait(released).expect("mutex");
            }
        }
        let rx = self.obs_rx.as_ref()?;
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        // Wake-driven: returns immediately on send; the bound only caps
        // stop latency, it never polls.
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => Some(result),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.obs_rx = None;
                None
            }
        }
    }

    fn shutdown(&mut self) -> Result<(), String> {
        if self
            .inner
            .shutdown_fail
            .lock()
            .expect("mutex")
            .contains(&self.pid)
        {
            return Err("fake teardown refused".to_owned());
        }
        Ok(())
    }
}

struct FakeInputFactory {
    rx: Mutex<Option<mpsc::Receiver<RawKeyEvent>>>,
    panic_on_next: AtomicBool,
    fail_create: AtomicBool,
    create_gate: Mutex<Option<Arc<Gate>>>,
}

impl InputSourceFactory for FakeInputFactory {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        if self.fail_create.load(Ordering::SeqCst) {
            return Err("fake input refused startup".to_owned());
        }
        // Creation gate (cloned out before waiting) lets tests rendezvous
        // with an input thread stuck inside factory creation.
        let gate = self.create_gate.lock().expect("mutex").clone();
        if let Some(gate) = gate {
            let mut released = gate.released.lock().expect("mutex");
            while !*released {
                released = gate.cond.wait(released).expect("mutex");
            }
        }
        let rx = self
            .rx
            .lock()
            .expect("mutex")
            .take()
            .ok_or_else(|| "fake input receiver already taken".to_owned())?;
        Ok(Box::new(FakeInputSource {
            rx,
            panic: self.panic_on_next.load(Ordering::SeqCst),
        }))
    }

    fn describe(&self) -> &'static str {
        "fake-channel-input"
    }
}

struct FakeInputSource {
    rx: mpsc::Receiver<RawKeyEvent>,
    panic: bool,
}

impl InputSource for FakeInputSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
        if self.panic {
            panic!("fake input panic");
        }
        loop {
            match self.rx.recv_timeout(Duration::from_millis(10)) {
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

// -- Fixture -------------------------------------------------------------------

#[allow(dead_code)]
struct Fixture {
    handle: RuntimeHandle,
    ax: Arc<FakeAxInner>,
    applied_rx: mpsc::Receiver<AppliedRecord>,
    entered_rx: mpsc::Receiver<EnteredRecord>,
    input_tx: mpsc::Sender<RawKeyEvent>,
}

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        worker_ready_timeout: Duration::from_secs(5),
        ..RuntimeConfig::default()
    }
}

fn start(config: RuntimeConfig) -> Fixture {
    let (applied_tx, applied_rx) = mpsc::channel::<AppliedRecord>();
    let (entered_tx, entered_rx) = mpsc::channel::<EnteredRecord>();
    let (input_tx, input_rx) = mpsc::channel::<RawKeyEvent>();
    let ax = Arc::new(FakeAxInner {
        applied_tx,
        entered_tx,
        gates: Mutex::new(BTreeMap::new()),
        create_gates: Mutex::new(BTreeMap::new()),
        poll_gates: Mutex::new(BTreeMap::new()),
        snapshots: Mutex::new(BTreeMap::new()),
        snapshot_fail: Mutex::new(BTreeSet::new()),
        obs_inboxes: Mutex::new(BTreeMap::new()),
        panic_pids: Mutex::new(BTreeSet::new()),
        fail_create: AtomicBool::new(false),
        timeout_once: Mutex::new(BTreeSet::new()),
        fail_status_once: Mutex::new(BTreeSet::new()),
        shutdown_fail: Mutex::new(BTreeSet::new()),
    });
    let input = Arc::new(FakeInputFactory {
        rx: Mutex::new(Some(input_rx)),
        panic_on_next: AtomicBool::new(false),
        fail_create: AtomicBool::new(false),
        create_gate: Mutex::new(None),
    });
    let handle = Runtime::start(
        config,
        Arc::new(FakeAxFactory {
            inner: Arc::clone(&ax),
        }),
        input,
    )
    .expect("runtime starts offline");
    Fixture {
        handle,
        ax,
        applied_rx,
        entered_rx,
        input_tx,
    }
}
/// Explicit snapshot content for one pid (startup + readback source).
/// Configuring — even with an empty vec — is what lets the worker publish.
fn configure_snapshot(fixture: &Fixture, pid: u32, windows: Vec<ObservedWindow>) {
    fixture
        .ax
        .snapshots
        .lock()
        .expect("mutex")
        .insert(pid, windows);
}

/// Install an autonomous observation inbox for one pid. Must be called
/// before launch (the owning worker takes it at creation). Returns the
/// sender the test uses to emit backend observations without ever calling
/// `submit_observation`.
fn autonomous_app(fixture: &Fixture, pid: u32) -> mpsc::Sender<Result<BackendSnapshot, String>> {
    let (tx, rx) = mpsc::channel::<Result<BackendSnapshot, String>>();
    fixture
        .ax
        .obs_inboxes
        .lock()
        .expect("mutex")
        .insert(pid, rx);
    tx
}

fn backend_snapshot(windows: Vec<ObservedWindow>, focused_wid: Option<u32>) -> BackendSnapshot {
    BackendSnapshot {
        windows,
        focused_wid,
    }
}
fn win(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
    ObservedWindow {
        wid,
        x,
        y,
        w,
        h,
        floating: false,
        fullscreen: false,
        hints: tiler_macos::runtime::WindowSizeHints::none(),
    }
}

fn obs(
    pid: u32,
    generation: u64,
    seq: u64,
    windows: Vec<ObservedWindow>,
    focused_wid: Option<u32>,
) -> AxObservation {
    AxObservation {
        pid,
        generation,
        seq,
        windows,
        focused_wid,
    }
}

fn flush(handle: &RuntimeHandle) {
    assert!(handle.flush(FLUSH_TIMEOUT), "engine flush timed out");
}
/// Poll diagnostics until `wanted` holds or the timeout expires. Polling
/// for an inevitable event is deterministic in outcome: the event must
/// arrive (threads were rendezvoused beforehand); only the wait varies.
fn wait_diagnostics(
    handle: &RuntimeHandle,
    wanted: impl Fn(&tiler_macos::runtime::RuntimeDiagnostics) -> bool,
    what: &str,
) -> tiler_macos::runtime::RuntimeDiagnostics {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let diagnostics = handle.diagnostics();
        if wanted(&diagnostics) {
            return diagnostics;
        }
        if std::time::Instant::now() >= deadline {
            panic!("diagnostics never showed {what}: {diagnostics:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_applied(fixture: &Fixture) -> AppliedRecord {
    fixture
        .applied_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("no applied frame arrived")
}

fn drain_applied(fixture: &Fixture) -> Vec<AppliedRecord> {
    let mut out = Vec::new();
    while let Ok(record) = fixture.applied_rx.try_recv() {
        out.push(record);
    }
    out
}
/// Park the engine at the first opened world flight (midflight
/// rendezvous): the flight signals opened and blocks until the returned
/// release is sent; later flights pass through untouched. Channel
/// rendezvous only — no sleeps, no live desktop.
fn park_first_flight(handle: &RuntimeHandle) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
    use tiler_macos::runtime::MidflightHook;
    let (flight_open_tx, flight_open_rx) = mpsc::channel::<()>();
    let (proceed_tx, proceed_rx) = mpsc::channel::<()>();
    let proceed_rx = Mutex::new(proceed_rx);
    let first = Arc::new(AtomicBool::new(true));
    let hook: MidflightHook = Arc::new(move || {
        if !first.swap(false, Ordering::SeqCst) {
            return;
        }
        let _ = flight_open_tx.send(());
        let _ = proceed_rx.lock().expect("mutex").recv();
    });
    handle.set_midflight_hook_for_test(Some(hook));
    (flight_open_rx, proceed_tx)
}

/// Collect applied records until every wanted window has arrived at least
/// once (deadline-bounded), then return the whole collected multiset
/// sorted. Flush fences engine jobs, not worker applies, so a bare drain
/// after a flush can race a slow worker and miss records; collecting
/// until the expected set lands is deterministic. Callers compare the
/// multiset exactly against the wanted set (plus a trailing empty drain):
/// takes are ordered, so any stale batch taken earlier shows its records
/// here, and any overwritten one shows in `frames_coalesced`.
fn collect_applied_exact(fixture: &Fixture, wants: &[&str]) -> Vec<String> {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    while wants.iter().any(|want| !seen.contains(*want)) {
        if std::time::Instant::now() >= deadline {
            break;
        }
        if let Ok(record) = fixture.applied_rx.recv_timeout(Duration::from_millis(50)) {
            seen.insert(record.window.clone());
            out.push(record.window);
        }
    }
    let mut drained: Vec<String> = drain_applied(fixture)
        .into_iter()
        .map(|record| record.window)
        .collect();
    out.append(&mut drained);
    out.sort_unstable();
    out
}

/// Windows in the latest desired plan, sorted.
fn desired_windows(handle: &RuntimeHandle) -> Vec<String> {
    let mut out: Vec<String> = handle
        .last_desired_geometry()
        .iter()
        .map(|frame| frame.window.clone())
        .collect();
    out.sort_unstable();
    out
}
fn shutdown_clean(fixture: &Fixture) {
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(
        report.outcome,
        ShutdownOutcome::Clean,
        "expected clean shutdown, got {report:?}"
    );
}
fn wait_desired_names(fixture: &Fixture, names: &[&str]) -> Vec<String> {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let desired = fixture.handle.last_desired_geometry();
        let mut got: Vec<String> = desired.iter().map(|f| f.window.clone()).collect();
        got.sort();
        let mut want: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        want.sort();
        if got == want {
            return got;
        }
        if std::time::Instant::now() >= deadline {
            panic!("desired geometry never converged to {want:?}: got {got:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
// -- Tests ---------------------------------------------------------------------

#[test]
fn redundant_frame_writes_skipped_once_observed() {
    let fixture = start(test_config());
    configure_snapshot(
        &fixture,
        APP_A,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
    );
    let obs_tx = autonomous_app(&fixture, APP_A);
    fixture.handle.launch_app(APP_A).expect("launch");
    // Startup actuates (desired tiling differs from observed): converge on
    // the first plan so the backend record matches the core plan.
    wait_desired_names(&fixture, &["1001_11", "1001_12"]);
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut seen: BTreeMap<String, (i32, i32, i32, i32)> = BTreeMap::new();
    loop {
        let desired = fixture.handle.last_desired_geometry();
        while let Ok(record) = fixture.applied_rx.try_recv() {
            seen.insert(record.window.clone(), record.rect);
        }
        if !desired.is_empty()
            && desired
                .iter()
                .all(|f| seen.get(&f.window) == Some(&(f.x, f.y, f.w, f.h)))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "first plan never actuated: {seen:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    drain_applied(&fixture);
    // Report the planned geometry back as observed truth: the next identical
    // plan carries no new information, so every write is skipped — truth
    // still flows (the observation publishes) while redundant native writes
    // do not.
    let desired = fixture.handle.last_desired_geometry();
    assert_eq!(desired.len(), 2);
    let observed: Vec<ObservedWindow> = desired
        .iter()
        .map(|f| {
            let (_, wid) = f.window.split_once('_').expect("minted identity");
            let (x, y, w, h) = (f.x, f.y, f.w, f.h);
            ObservedWindow {
                wid: wid.parse().expect("numeric wid"),
                x,
                y,
                w,
                h,
                floating: false,
                fullscreen: false,
                hints: tiler_macos::runtime::WindowSizeHints::none(),
            }
        })
        .collect();
    configure_snapshot(&fixture, APP_A, observed.clone());
    obs_tx
        .send(Ok(backend_snapshot(observed, None)))
        .expect("backend send");
    // Deterministic rendezvous on the skip counter (not on timing): both
    // re-planned commands must skip their native write.
    wait_diagnostics(
        &fixture.handle,
        |d| d.frames_redundant_skipped >= 2,
        "redundant skips",
    );
    flush(&fixture.handle);
    assert!(
        fixture.applied_rx.try_recv().is_err(),
        "identical geometry needs no native write"
    );
    assert!(
        fixture.handle.diagnostics().observations_published >= 2,
        "truth still publishes while writes skip"
    );
    shutdown_clean(&fixture);
}

#[test]
fn empty_observation_removes_all_and_cancels_batches() {
    let fixture = start(test_config());
    configure_snapshot(&fixture, APP_A, vec![win(11, 0, 0, 960, 1080)]);
    let obs_tx = autonomous_app(&fixture, APP_A);
    fixture.handle.launch_app(APP_A).expect("launch");
    wait_desired_names(&fixture, &["1001_11"]);
    // Rendezvous with the first plan's actuation before draining: without
    // it a still-in-flight first apply would be mistaken for post-removal
    // work below.
    let _ = wait_applied(&fixture);
    drain_applied(&fixture);
    // All windows gone: the whole empty desired set supersedes pending
    // frame state (cancelling the batch), and the configured empty snapshot
    // publishes as honest truth — an explicit empty, never a failure
    // masquerading as empty.
    configure_snapshot(&fixture, APP_A, Vec::new());
    obs_tx
        .send(Ok(backend_snapshot(Vec::new(), None)))
        .expect("backend send");
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture.handle.last_desired_geometry().is_empty()
            && fixture.handle.diagnostics().observations_published >= 2
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "removal never converged: {:?}",
            fixture.handle.last_desired_geometry()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    flush(&fixture.handle);
    assert!(
        fixture.applied_rx.try_recv().is_err(),
        "an empty plan actuates nothing"
    );
    shutdown_clean(&fixture);
}

#[test]
fn midflight_input_stales_the_flight_and_newest_applies_alone() {
    // The current-watermark fence end to end through the production path:
    // input submitted while a world request is outstanding advances the
    // shared mark; the in-flight reply refuses as stale-observation with
    // no native emission from it (no frames, no focus outbox); a second
    // queued observation still refuses at the emission gate (its snapshot
    // lacks the newer queued truth); only the newest converges, exactly
    // once. Channel rendezvous only — no sleeps, no live desktop.
    // (Generation and world-revision fencing ride the same `accept_reply`
    // gate, proven by the `world_sequencer` unit matrix.)
    //
    // The absence assertions are the load-bearing ones: no applied frame
    // may carry an intermediate plan, and no coalescing may hide one —
    // every stale batch is either taken (then its records show) or
    // overwritten (then the counter shows), so both together prove no
    // stale setter ran, not merely that the end state converged.
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    // Baseline truth, flushed to applied (proves the worker took the
    // slot batch, so the slot is empty before the race).
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let baseline = wait_applied(&fixture);
    assert_eq!(baseline.window, "1001_11");
    assert!(drain_applied(&fixture).is_empty());

    let (flight_open_rx, proceed_tx) = park_first_flight(&fixture.handle);

    // Newer truth arrives on the engine thread; the flight opens over it
    // and blocks mid-flight with the ticket stamped.
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            2,
            vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flight_open_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("flight must open over the racing observation");
    // TWO even newer truths race in while the request is outstanding: the
    // shared mark advances twice past the flight's stamped observation,
    // and both stay queued while the engine is parked.
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            3,
            vec![
                win(11, 0, 0, 640, 1080),
                win(12, 640, 0, 640, 1080),
                win(13, 1280, 0, 640, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            4,
            vec![
                win(11, 0, 0, 480, 1080),
                win(12, 480, 0, 480, 1080),
                win(13, 960, 0, 480, 1080),
                win(14, 1440, 0, 480, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    proceed_tx
        .send(())
        .expect("engine must be parked mid-flight");
    flush(&fixture.handle);
    fixture.handle.set_midflight_hook_for_test(None);

    // Both stale replies refused (counted) instead of actuating over the
    // racing input — the first at acceptance, the second at the emission
    // gate — and the refusal lands in the ring BEFORE the later accepted
    // dispatch, proving emission order, not just totals.
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-refused-stale-observation")
            .copied()
            .unwrap_or(0),
        2,
        "both superseded dispatches must refuse: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        2,
        "baseline plus newest only must apply: {:?}",
        diagnostics.core_replies
    );
    let log = fixture.handle.log_snapshot();
    let stale_at = log
        .iter()
        .position(|e| e.kind == "core-reply" && e.outcome == "stale-observation")
        .expect("ring must show the stale refusal");
    let accepted_after = log[stale_at + 1..]
        .iter()
        .filter(|e| e.kind == "core-reply" && e.outcome == "reconcile-accepted")
        .count();
    assert_eq!(
        accepted_after, 1,
        "only the newest dispatch may apply after the refusals: {log:?}"
    );
    // No stale setter ran: the newest plan emitted its four frames into an
    // empty slot (nothing coalesced away a hidden stale batch), and every
    // applied frame after the baseline is exactly the newest window set.
    assert_eq!(diagnostics.frames_emitted, 1 + 4);
    assert_eq!(
        diagnostics.frames_coalesced, 0,
        "no hidden stale batch may have been overwritten: {diagnostics:?}"
    );
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12", "1001_13", "1001_14"]),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned(),
            "1001_14".to_owned()
        ],
        "only the newest plan may reach the worker"
    );
    // The newest truth — and only the newest — is the retained plan.
    // No native focus dispatches for any admission newcomer: observations
    // keep observed-focus authority (the superseded replies' spent intents
    // `1001_12`/`1001_13` must never reach a native setter either).
    assert_eq!(
        desired_windows(&fixture.handle),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned(),
            "1001_14".to_owned()
        ]
    );
    assert!(
        fixture.handle.drain_focus_requests().is_empty(),
        "observation-only replies must never dispatch native focus"
    );
    shutdown_clean(&fixture);
}

#[test]
fn post_accept_input_suppresses_stale_emission() {
    // The emission-coherence gate end to end: a reply that the sequencer
    // accepts still must not reach native setters when input lands between
    // acceptance and emission. The post-accept rendezvous parks the engine
    // after acceptance with no locks held; the test advances the watermark
    // over channels, then releases. The gated reply spends nothing and
    // emits nothing; the queued successor converges newest-only.
    use tiler_macos::runtime::PostAcceptHook;

    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let baseline = wait_applied(&fixture);
    assert_eq!(baseline.window, "1001_11");
    assert!(drain_applied(&fixture).is_empty());

    // Park once at the first acceptance; later replies pass through.
    let (accepted_tx, accepted_rx) = mpsc::channel::<()>();
    let (proceed_tx, proceed_rx) = mpsc::channel::<()>();
    let proceed_rx = Mutex::new(proceed_rx);
    let first = Arc::new(AtomicBool::new(true));
    let hook: PostAcceptHook = Arc::new(move || {
        if !first.swap(false, Ordering::SeqCst) {
            return;
        }
        let _ = accepted_tx.send(());
        let _ = proceed_rx.lock().expect("mutex").recv();
    });
    fixture.handle.set_post_accept_hook_for_test(Some(hook));

    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            2,
            vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    accepted_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("reply must accept before emission");
    // Newer truth lands after acceptance but before emission: the accepted
    // snapshot is superseded and must never reach the worker.
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            3,
            vec![
                win(11, 0, 0, 640, 1080),
                win(12, 640, 0, 640, 1080),
                win(13, 1280, 0, 640, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    proceed_tx
        .send(())
        .expect("engine must be parked post-accept");
    flush(&fixture.handle);
    fixture.handle.set_post_accept_hook_for_test(None);

    // The accepted-but-superseded reply refused at the gate (counted as
    // stale-observation, spending nothing); only baseline plus the newest
    // applied, with no hidden stale batch overwritten or taken.
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-refused-stale-observation")
            .copied()
            .unwrap_or(0),
        1,
        "post-accept staleness must refuse: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        2,
        "baseline plus newest only must apply: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(diagnostics.frames_emitted, 1 + 3);
    assert_eq!(
        diagnostics.frames_coalesced, 0,
        "no hidden stale batch may have been overwritten: {diagnostics:?}"
    );
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12", "1001_13"]),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned()
        ],
        "the superseded plan may never reach the worker"
    );
    assert_eq!(
        desired_windows(&fixture.handle),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned()
        ]
    );
    // No native focus dispatches for any admission newcomer: the
    // superseded reply's spent intent never reached a native setter, and
    // neither did the newest plan's (observations keep observed-focus
    // authority).
    assert!(
        fixture.handle.drain_focus_requests().is_empty(),
        "observation-only replies must never dispatch native focus"
    );
    shutdown_clean(&fixture);
}

#[test]
fn mark_without_job_fences_and_never_wedges() {
    // Mark-without-job audit end to end: `update_focus_evidence` advances
    // the watermark with no queued job. A mid-flight mark still fences the
    // outstanding reply (counted refusal, no emission from it), and with
    // no honest successor coming, one deferred observation-only retry
    // recovers the fenced truth instead of stalling: sequencing never
    // wedges, and the next observation still converges newest-only.
    use tiler_macos::runtime::FocusEvidence;

    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let baseline = wait_applied(&fixture);
    assert_eq!(baseline.window, "1001_11");
    assert!(drain_applied(&fixture).is_empty());

    let (flight_open_rx, proceed_tx) = park_first_flight(&fixture.handle);

    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            2,
            vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flight_open_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("flight must open");
    // No-job mark mid-flight: fences the outstanding reply without
    // queueing any successor.
    fixture.handle.update_focus_evidence(FocusEvidence {
        frontmost_pid: Some(APP_A),
        frontmost_bundle: None,
        ax_focus: Some((APP_A, Some(11))),
    });
    proceed_tx
        .send(())
        .expect("engine must be parked mid-flight");
    flush(&fixture.handle);
    fixture.handle.set_midflight_hook_for_test(None);

    // The fenced reply refused (its emission would have been stale); the
    // deferred retry then applied the same truth honestly — nothing newer
    // exists, and the evidence mark carries no incorporable truth for the
    // legacy path. Sequencing proceeds: the next observation opens a
    // fresh flight and applies newest-only.
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-refused-stale-observation")
            .copied()
            .unwrap_or(0),
        1,
        "the fenced reply must refuse: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        2,
        "baseline plus the recovered retry must apply: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12"]),
        vec!["1001_11".to_owned(), "1001_12".to_owned()],
        "the recovered retry emits the fenced truth"
    );
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            3,
            vec![
                win(11, 0, 0, 640, 1080),
                win(12, 640, 0, 640, 1080),
                win(13, 1280, 0, 640, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        3,
        "baseline plus retry plus newest must apply: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(diagnostics.frames_coalesced, 0);
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12", "1001_13"]),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned()
        ],
        "only the newest plan may reach the worker"
    );
    shutdown_clean(&fixture);
}

#[test]
fn saturated_queue_overmarks_but_newest_and_retry_converge() {
    // Enqueue-failure edge end to end: with the engine parked mid-flight,
    // submissions fill the bounded data lane (default depth 64) and one
    // more refuses `EngineQueueFull`. The refused submit over-advanced the
    // watermark (conservative: invalidate runs before enqueue); the
    // coherent dispatch claims the dead mark with the newest snapshot, so
    // sequencing never wedges: every superseded dispatch refuses with no
    // stale emission, the newest queued observation applies exactly once,
    // and a retried observation afterwards still applies.
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let baseline = wait_applied(&fixture);
    assert_eq!(baseline.window, "1001_11");
    assert!(drain_applied(&fixture).is_empty());

    let (flight_open_rx, proceed_tx) = park_first_flight(&fixture.handle);
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            2,
            vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flight_open_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("flight must open");
    // Fill the lane: 64 queued behind the dequeued flight. Every queued
    // report carries a distinct newcomer so none collapses to a core
    // duplicate (each must reach the stale gate, not the dedupe lane).
    for i in 1..=64u32 {
        assert_eq!(
            fixture.handle.submit_observation(obs(
                APP_A,
                generation,
                2 + u64::from(i),
                vec![
                    win(11, 0, 0, 640, 1080),
                    win(12, 640, 0, 640, 1080),
                    win(100 + i, 1280, 0, 640, 1080),
                ],
                Some(11),
            )),
            SubmitOutcome::Accepted
        );
    }
    // One more refuses: the mark over-advanced for a dropped job.
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            67,
            vec![
                win(11, 0, 0, 640, 1080),
                win(12, 640, 0, 640, 1080),
                win(199, 1280, 0, 640, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::EngineQueueFull
    );
    proceed_tx
        .send(())
        .expect("engine must be parked mid-flight");
    flush(&fixture.handle);
    fixture.handle.set_midflight_hook_for_test(None);

    // The parked flight plus all 63 superseded queued dispatches refused;
    // only the newest queued observation applied — claiming the dead mark
    // with it (its truth was never queued, so nothing newer exists to be
    // stale against; the worker retries dropped truth with a fresh mark).
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-refused-stale-observation")
            .copied()
            .unwrap_or(0),
        64,
        "every superseded dispatch must refuse: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        2,
        "baseline plus newest only must apply: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(diagnostics.frames_emitted, 1 + 3);
    assert_eq!(
        diagnostics.frames_coalesced, 0,
        "no hidden stale batch may have been overwritten: {diagnostics:?}"
    );
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12", "1001_164"]),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_164".to_owned()
        ],
        "only the newest queued plan may reach the worker"
    );
    assert_eq!(
        desired_windows(&fixture.handle),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_164".to_owned()
        ]
    );
    // The retry arrives after the applied batch drained (channel receipt
    // proves the worker took the slot batch, so the slot is empty).
    // It applies normally, proving the coherence count compensated
    // instead of wedging open (a leak refuses this forever).
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            68,
            vec![
                win(11, 0, 0, 640, 1080),
                win(12, 640, 0, 640, 1080),
                win(200, 1280, 0, 640, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        3,
        "the retry must apply: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(diagnostics.frames_coalesced, 0);
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12", "1001_200"]),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_200".to_owned()
        ],
        "the retry plan may reach the worker"
    );
    assert_eq!(
        desired_windows(&fixture.handle),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_200".to_owned()
        ]
    );
    shutdown_clean(&fixture);
}

#[test]
fn stale_queued_observation_never_blocks_newest() {
    // Peek-and-classify end to end: a queued observation that will drop
    // as out-of-order must not suppress the fresher dispatch ahead of its
    // own drop — a drop dispatches no successor, so blocking on it would
    // stall convergence with no recovery. The stale mark still fences the
    // parked flight; the fresh successor applies newest-only.
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    let baseline = wait_applied(&fixture);
    assert_eq!(baseline.window, "1001_11");
    assert!(drain_applied(&fixture).is_empty());

    let (flight_open_rx, proceed_tx) = park_first_flight(&fixture.handle);
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            2,
            vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    flight_open_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("flight must open");
    // Fresh truth races in, then a duplicate of the already-incorporated
    // baseline (per-app sequence 1): the engine will drop it as
    // out-of-order, but its submit already moved the shared mark.
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            3,
            vec![
                win(11, 0, 0, 640, 1080),
                win(12, 640, 0, 640, 1080),
                win(13, 1280, 0, 640, 1080),
            ],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            generation,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        )),
        SubmitOutcome::Accepted
    );
    proceed_tx
        .send(())
        .expect("engine must be parked mid-flight");
    flush(&fixture.handle);
    fixture.handle.set_midflight_hook_for_test(None);

    // The parked flight refused (fenced by both later marks); the fresh
    // successor applied despite the stale job still queued at its gate;
    // the stale job then dropped without dispatching.
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-refused-stale-observation")
            .copied()
            .unwrap_or(0),
        1,
        "only the parked flight must refuse: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0),
        2,
        "baseline plus newest must apply: {:?}",
        diagnostics.core_replies
    );
    assert!(
        diagnostics.stale_dropped >= 1,
        "the duplicate must drop out-of-order: {diagnostics:?}"
    );
    assert_eq!(diagnostics.frames_coalesced, 0);
    assert_eq!(
        collect_applied_exact(&fixture, &["1001_11", "1001_12", "1001_13"]),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned()
        ],
        "only the newest plan may reach the worker"
    );
    assert_eq!(
        desired_windows(&fixture.handle),
        vec![
            "1001_11".to_owned(),
            "1001_12".to_owned(),
            "1001_13".to_owned()
        ]
    );
    shutdown_clean(&fixture);
}
