//! Phase 1 threading/lifecycle integration tests (offline, portable).
//!
//! Deterministic fakes drive the runtime through its public typed handoffs:
//! barriers and channels synchronize threads, never timing luck. Every test
//! observes actual core outputs (frame plans, focus/move replies) rather
//! than bare counters. No live AX, no taps, no Spaces, no desktop effects.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use tiler_macos::config::ProbeModifier;
use tiler_macos::input::cg_flag_bits;
use tiler_macos::input_match::TapKeyKind;
use tiler_macos::runtime::{
    AxCompletion, AxObservation, FrameCommand, InputDirection, ObservedWindow, Runtime,
    RuntimeConfig, RuntimeHandle, ShutdownOutcome, StartupError, SubmitOutcome, TerminateOutcome,
    WorkerReadiness,
};
use tiler_macos::runtime_backend::{
    AxBackend, AxBackendFactory, BackendSnapshot, InputSource, InputSourceFactory, RawKeyEvent,
};
use tiler_macos::runtime_log::{LogEvent, LogSink};

// -- Fakes ---------------------------------------------------------------------

const APP_A: u32 = 1001;
const APP_B: u32 = 1002;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct AppliedRecord {
    pid: u32,
    generation: u64,
    cmd_id: u64,
    window: String,
    rect: (i32, i32, i32, i32),
}

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

fn gate_app(fixture: &Fixture, pid: u32) -> Arc<Gate> {
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        cond: Condvar::new(),
    });
    fixture
        .ax
        .gates
        .lock()
        .expect("mutex")
        .insert(pid, Arc::clone(&gate));
    gate
}

fn release_gate(gate: &Gate) {
    *gate.released.lock().expect("mutex") = true;
    gate.cond.notify_all();
}

fn gate_create_app(fixture: &Fixture, pid: u32) -> Arc<Gate> {
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        cond: Condvar::new(),
    });
    fixture
        .ax
        .create_gates
        .lock()
        .expect("mutex")
        .insert(pid, Arc::clone(&gate));
    gate
}

fn gate_poll_app(fixture: &Fixture, pid: u32) -> Arc<Gate> {
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        cond: Condvar::new(),
    });
    fixture
        .ax
        .poll_gates
        .lock()
        .expect("mutex")
        .insert(pid, Arc::clone(&gate));
    gate
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

fn wait_entered(fixture: &Fixture, pid: u32) -> EnteredRecord {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("no AX entry for pid {pid}");
        }
        match fixture.entered_rx.recv_timeout(remaining) {
            Ok(record) if record.pid == pid => return record,
            Ok(_) => continue,
            Err(_) => panic!("no AX entry for pid {pid}"),
        }
    }
}

fn drain_entered(fixture: &Fixture) {
    while fixture.entered_rx.try_recv().is_ok() {}
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

/// Collect applied records until every wanted per-PID command id has
/// arrived (deadline-bounded). Single-plan lifetimes only: every emission is
/// applied exactly once there, so the rendezvous is exact and stale reads
/// are impossible. (Multi-plan tests use rect-convergence instead, because
/// latest-wins may coalesce takes.) Records for other PIDs are kept, never
/// dropped.
fn collect_for_pids(fixture: &Fixture, wants: &[(u32, Vec<u64>)]) -> Vec<AppliedRecord> {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut out = Vec::new();
    let mut seen: BTreeMap<u32, BTreeSet<u64>> = BTreeMap::new();
    let satisfied = |seen: &BTreeMap<u32, BTreeSet<u64>>| {
        wants.iter().all(|(pid, ids)| {
            ids.iter()
                .all(|id| seen.get(pid).is_some_and(|set| set.contains(id)))
        })
    };
    while !satisfied(&seen) && std::time::Instant::now() < deadline {
        if let Ok(record) = fixture.applied_rx.recv_timeout(Duration::from_millis(50)) {
            seen.entry(record.pid).or_default().insert(record.cmd_id);
            out.push(record);
        }
    }
    assert!(
        satisfied(&seen),
        "wanted applies never arrived (wants {wants:?}, seen {seen:?})"
    );
    out
}

fn flags(modifier: ProbeModifier) -> u64 {
    cg_flag_bits(modifier).0
}

fn key(kind: TapKeyKind, key_flags: u64, key_code: u32) -> RawKeyEvent {
    RawKeyEvent {
        kind,
        flags: key_flags,
        key_code,
    }
}

fn focus_key() -> RawKeyEvent {
    key(TapKeyKind::KeyDown, flags(ProbeModifier::Ctrl), 0x04)
}

fn move_key() -> RawKeyEvent {
    key(TapKeyKind::KeyDown, flags(ProbeModifier::Ctrl), 0x05)
}

/// Wait until the input owner has classified `forwarded`/`ignored` events,
/// then flush the engine. The wait is required: input sources forward
/// asynchronously behind the test thread, so a bare flush would race
/// forwarding and prove nothing about the sent events.
fn send_input_and_flush(fixture: &Fixture, events: &[RawKeyEvent], forwarded: u64, ignored: u64) {
    for event in events {
        fixture.input_tx.send(*event).expect("input send");
    }
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_forwarded >= forwarded && d.input_ignored >= ignored,
        "input classification",
    );
    flush(&fixture.handle);
}

/// Relaunch after terminate/stop: the previous generation may need a
/// moment to exit its in-flight apply even though stop is set. Retry only
/// on the honest overlap refusal, with a bounded deadline; any other
/// outcome (including success) returns immediately.
fn relaunch(handle: &RuntimeHandle, pid: u32) -> u64 {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        match handle.launch_app(pid) {
            Ok(generation) => return generation,
            Err(StartupError::PreviousGenerationRunning { .. }) => {
                if std::time::Instant::now() >= deadline {
                    panic!("previous generation for pid {pid} never exited");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(other) => panic!("relaunch failed: {other:?}"),
        }
    }
}

fn shutdown_clean(fixture: &Fixture) {
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(
        report.outcome,
        ShutdownOutcome::Clean,
        "expected clean shutdown, got {report:?}"
    );
}

// -- Tests ---------------------------------------------------------------------

#[test]
fn reconcile_drives_frame_plan_to_workers() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(fixture.handle.app_generation(APP_A), Some(generation));

    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);

    // Actual core output: two desired frames, one per window.
    let desired = fixture.handle.last_desired_geometry();
    assert_eq!(desired.len(), 2, "core must plan both windows: {desired:?}");
    let mut names: Vec<&str> = desired.iter().map(|f| f.window.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["1001_11", "1001_12"]);

    // Workers actuate exactly the planned frames.
    let first = wait_applied(&fixture);
    let second = wait_applied(&fixture);
    let mut applied = [first, second];
    applied.sort_by(|a, b| a.window.cmp(&b.window));
    // Per-app command sequencing starts at 1 for the first plan.
    assert_eq!(
        applied.iter().map(|r| r.cmd_id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    for (record, frame) in applied.iter().zip(desired.iter()) {
        assert_eq!(record.window, frame.window);
        assert_eq!(
            record.rect,
            (frame.x, frame.y, frame.w, frame.h),
            "worker must apply the planned rect"
        );
        assert_eq!(record.generation, generation);
    }

    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics.core_replies.contains_key("reconcile-tiled")
            || diagnostics
                .core_replies
                .contains_key("reconcile-projection"),
        "expected a reconcile plan reply: {:?}",
        diagnostics.core_replies
    );
    assert!(diagnostics.frames_emitted >= 2);
    // Reconcile plans geometry but never dispatches native focus.
    assert!(
        fixture.handle.drain_focus_requests().is_empty(),
        "reconcile must not dispatch focus"
    );
    shutdown_clean(&fixture);
}

#[test]
fn hung_app_cannot_stall_engine_input_or_other_app() {
    let fixture = start(test_config());
    let gate = gate_app(&fixture, APP_A);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch A");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");

    // A takes its first frame set and blocks inside AX. The rendezvous is
    // deterministic: nothing else is submitted until A entered apply.
    fixture.handle.submit_observation(obs(
        APP_A,
        gen_a,
        1,
        vec![win(11, 0, 0, 480, 1080), win(12, 480, 0, 480, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let entered_a = wait_entered(&fixture, APP_A);
    assert_eq!(entered_a.pid, APP_A);
    assert_eq!(entered_a.cmd_id, 1, "first plan starts the sequence");

    // While A is hung, the engine, input, and B all progress. The flush
    // itself proves the engine is not stalled by A.
    fixture.handle.submit_observation(obs(
        APP_B,
        gen_b,
        1,
        vec![win(21, 960, 0, 480, 1080), win(22, 1440, 0, 480, 1080)],
        Some(21),
    ));
    send_input_and_flush(&fixture, &[focus_key()], 1, 0);

    // B progresses: its frames are applied while A is still blocked.
    let mut b_windows: BTreeSet<String> = BTreeSet::new();
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    while b_windows.len() < 2 && std::time::Instant::now() < deadline {
        if let Ok(record) = fixture.applied_rx.recv_timeout(Duration::from_millis(50)) {
            assert_eq!(record.pid, APP_B, "only B may apply while A is hung");
            b_windows.insert(record.window);
        }
    }
    assert_eq!(b_windows.len(), 2, "B must fully progress while A is hung");

    let diagnostics = fixture.handle.diagnostics();
    assert!(diagnostics.input_forwarded >= 1, "input must progress");
    assert!(diagnostics.engine_jobs_processed >= 4);

    // Release A: it finishes its in-flight set, then actuates every newer
    // set until it converges to the latest desired state.
    release_gate(&gate);
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut seen: BTreeMap<String, (i32, i32, i32, i32)> = BTreeMap::new();
    loop {
        let desired = fixture.handle.last_desired_geometry();
        while let Ok(record) = fixture.applied_rx.try_recv() {
            if record.pid == APP_A {
                seen.insert(record.window, record.rect);
            }
        }
        let converged = !desired.is_empty()
            && desired
                .iter()
                .filter(|f| f.window.starts_with("1001_"))
                .all(|f| seen.get(&f.window) == Some(&(f.x, f.y, f.w, f.h)));
        if converged {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("released app never converged: desired={desired:?} seen={seen:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    shutdown_clean(&fixture);
}

#[test]
fn terminate_mid_command_rejects_late_result() {
    let fixture = start(test_config());
    let gate = gate_app(&fixture, APP_A);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch A");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");

    fixture.handle.submit_observation(obs(
        APP_A,
        gen_a,
        1,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    // A has taken the two-command desired set and is blocked inside the
    // first apply.
    let entered = wait_entered(&fixture, APP_A);
    assert_eq!(entered.pid, APP_A);
    assert_eq!(entered.cmd_id, 1);

    let TerminateOutcome::Terminated { generation } = fixture.handle.terminate_app(APP_A) else {
        panic!("terminate must succeed");
    };
    assert_eq!(generation, gen_a);
    assert_eq!(fixture.handle.app_generation(APP_A), Some(generation));

    // Per-command fence: the in-flight first call may finish, but the
    // remainder of the taken batch must never start. Exactly one backend
    // dispatch truth arrives for A.
    release_gate(&gate);
    let late = wait_applied(&fixture);
    assert_eq!(late.pid, APP_A, "backend dispatch truth still arrives");
    // The finished call reports honestly, but the engine rejects the late
    // result for the terminated generation — and the fenced remainder is
    // invalidated worker-side. Both are asynchronous: the barrier proves the
    // result lane drained, and the counter proves the remainder never ran.
    flush(&fixture.handle);
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.terminated_invalidated >= 2,
        "late-result and fenced-remainder invalidation",
    );
    assert!(
        diagnostics.terminated_invalidated >= 2,
        "late result and fenced remainder must be invalidated: {diagnostics:?}"
    );
    assert!(
        fixture.applied_rx.try_recv().is_err(),
        "no second command from the taken batch may start after stop"
    );

    // Unaffected apps remain operational.
    fixture.handle.submit_observation(obs(
        APP_B,
        gen_b,
        1,
        vec![win(21, 0, 0, 1920, 1080)],
        Some(21),
    ));
    flush(&fixture.handle);
    let applied_b = wait_applied(&fixture);
    assert_eq!(applied_b.pid, APP_B);
    shutdown_clean(&fixture);
}

#[test]
fn out_of_order_observations_cannot_regress_state() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    // Newer sequence first: two windows.
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        2,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    // Older sequence arrives late and must be rejected.
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);

    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics.stale_dropped >= 1,
        "out-of-order observation must be rejected: {diagnostics:?}"
    );
    // Actual core state still reflects the newer observation.
    let desired = fixture.handle.last_desired_geometry();
    assert_eq!(
        desired.len(),
        2,
        "plan must still cover both windows: {desired:?}"
    );
    let applied = vec![wait_applied(&fixture), wait_applied(&fixture)];
    assert!(
        applied
            .iter()
            .all(|r| r.window == "1001_11" || r.window == "1001_12"),
        "only the newer observation may actuate: {applied:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn stop_during_blocked_work_is_bounded_then_reaps() {
    let fixture = start(test_config());
    let gate = gate_app(&fixture, APP_A);
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let _ = wait_entered(&fixture, APP_A);

    // Bounded dirty shutdown while the worker is blocked in AX.
    let start = std::time::Instant::now();
    let report = fixture.handle.shutdown(Duration::from_millis(300));
    assert_eq!(report.outcome, ShutdownOutcome::DirtyTimeout, "{report:?}");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "shutdown must respect the deadline: {:?}",
        start.elapsed()
    );
    assert!(report.retained >= 1, "timed-out handle must be retained");
    assert!(!fixture.handle.is_running());

    // Admission is closed after stop, independent of queue state.
    assert_eq!(
        fixture
            .handle
            .submit_observation(obs(APP_A, generation, 2, vec![], None)),
        SubmitOutcome::AdmissionClosed
    );

    // The blocked generation still executes: replacement is refused.
    // (Admission is closed here, so this is covered by the flag; the
    // overlapping-replacement path is exercised in its own test.)

    // Release the backend; the retained handle becomes reapeable.
    release_gate(&gate);
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let reap = loop {
        let reap = fixture.handle.reap();
        if reap.remaining == 0 || std::time::Instant::now() >= deadline {
            break reap;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(reap.remaining, 0, "released handle must reap: {reap:?}");
    assert!(reap.reaped >= 1);

    // The terminal report is cached exactly: a repeated stop replays it
    // instead of hiding the timeout behind `AlreadyStopped`.
    let again = fixture.handle.shutdown(Duration::from_secs(1));
    assert_eq!(again, report, "terminal report must replay exactly");
}

#[test]
fn idle_never_terminates_and_wakes_on_new_data() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    std::thread::sleep(Duration::from_millis(200));
    assert!(fixture.handle.is_running(), "idle runtime must stay alive");
    assert!(
        fixture.handle.flush(FLUSH_TIMEOUT),
        "idle runtime must wake"
    );

    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let applied = wait_applied(&fixture);
    assert_eq!(applied.window, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn ax_panic_is_never_a_clean_stop() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.ax.panic_pids.lock().expect("mutex").insert(APP_A);
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    // Deterministic rendezvous: the worker entered apply and will panic.
    let _ = wait_entered(&fixture, APP_A);

    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Panic, "{report:?}");
    assert!(report.panics >= 1);
}

#[test]
fn input_panic_is_never_a_clean_stop() {
    let (applied_tx, _applied_rx) = mpsc::channel::<AppliedRecord>();
    let (entered_tx, _entered_rx) = mpsc::channel::<EnteredRecord>();
    let (_input_tx, input_rx) = mpsc::channel::<RawKeyEvent>();
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
        panic_on_next: AtomicBool::new(true),
        fail_create: AtomicBool::new(false),
        create_gate: Mutex::new(None),
    });
    let handle = Runtime::start(test_config(), Arc::new(FakeAxFactory { inner: ax }), input)
        .expect("runtime starts");
    // The input thread panics on its first poll, long before shutdown.
    std::thread::sleep(Duration::from_millis(200));
    let report = handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Panic, "{report:?}");
}

#[test]
fn engine_panic_is_never_a_clean_stop() {
    let fixture = start(test_config());
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.inject_engine_panic_for_test());
    // The engine is dead, so the FIFO barrier never completes.
    assert!(
        !fixture.handle.flush(Duration::from_secs(2)),
        "flush must fail once the engine panicked"
    );
    // Admission to a dead engine is closed, and the death reads as stopped.
    assert!(!fixture.handle.is_running(), "dead engine reads stopped");
    assert_eq!(
        fixture
            .handle
            .submit_observation(obs(APP_A, 1, 1, vec![], None)),
        SubmitOutcome::AdmissionClosed,
        "admission to a dead engine stays closed"
    );
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Panic, "{report:?}");
}

#[test]
fn honest_startup_failures_leave_runtime_usable() {
    let (applied_tx, _applied_rx) = mpsc::channel::<AppliedRecord>();
    let (entered_tx, _entered_rx) = mpsc::channel::<EnteredRecord>();
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
        fail_create: AtomicBool::new(true),
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
        test_config(),
        Arc::new(FakeAxFactory {
            inner: Arc::clone(&ax),
        }),
        input,
    )
    .expect("runtime starts");

    // Backend refusal fails the launch honestly.
    assert!(matches!(
        handle.launch_app(APP_A),
        Err(StartupError::BackendRefused(_))
    ));
    // Reserved PIDs fail before any thread exists.
    assert!(handle.launch_app(0).is_err());
    // Observations for unknown apps are refused, not queued.
    assert_eq!(
        handle.submit_observation(obs(APP_A, 0, 1, vec![], None)),
        SubmitOutcome::UnknownApp
    );

    // Clearing the fault makes the same PID launchable: no leaked state.
    ax.fail_create.store(false, Ordering::SeqCst);
    handle
        .launch_app(APP_A)
        .expect("launch works after fault clears");
    let _ = input_tx;
    let report = handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn input_startup_failure_cleans_up_the_engine() {
    let (applied_tx, _applied_rx) = mpsc::channel::<AppliedRecord>();
    let (entered_tx, _entered_rx) = mpsc::channel::<EnteredRecord>();
    let (_input_tx, input_rx) = mpsc::channel::<RawKeyEvent>();
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
        fail_create: AtomicBool::new(true),
        create_gate: Mutex::new(None),
    });
    let failure = match Runtime::start(test_config(), Arc::new(FakeAxFactory { inner: ax }), input)
    {
        Err(failure) => failure,
        Ok(_) => panic!("refused input must fail startup"),
    };
    assert!(
        matches!(failure.error, StartupError::InputRefused(_)),
        "expected input refusal, got {:?}",
        failure.error
    );
    // Bounded cleanup already ran: the refusal is the error, and the
    // cleanup report is honest (every owner exited promptly, so the
    // teardown itself is clean) with nothing left to reap.
    assert_eq!(
        failure.report.outcome,
        ShutdownOutcome::Clean,
        "prompt refusal must tear down cleanly: {:?}",
        failure.report
    );
    let reap = failure.handle.reap();
    assert_eq!(reap.remaining, 0, "refused start must reap: {reap:?}");
}

#[test]
fn focus_saturation_drops_counted_and_survives() {
    let config = RuntimeConfig {
        focus_outbox_depth: 1,
        ..test_config()
    };
    let fixture = start(config);
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    // Three windows in a row so two consecutive focus-right steps both
    // resolve through the core.
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![
            win(11, 0, 0, 640, 1080),
            win(12, 640, 0, 640, 1080),
            win(13, 1280, 0, 640, 1080),
        ],
        Some(11),
    ));
    flush(&fixture.handle);
    drain_applied(&fixture);

    // Two focus replies, one outbox slot: the second request must drop
    // with a counted outcome instead of blocking actuation.
    send_input_and_flush(&fixture, &[focus_key(), focus_key()], 2, 0);

    let diagnostics = fixture.handle.diagnostics();
    let focus_replies: u64 = diagnostics
        .core_replies
        .iter()
        .filter(|(k, _)| k.starts_with("focus-"))
        .map(|(_, v)| v)
        .sum();
    assert_eq!(
        focus_replies, 2,
        "both focus steps reach the core: {diagnostics:?}"
    );
    assert_eq!(diagnostics.focus_dispatched, 1);
    assert_eq!(diagnostics.focus_dropped_full, 1);

    let requests = fixture.handle.drain_focus_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].dispatched);
    // Dispatch is never verified focus.
    assert!(!requests[0].verified);

    // The runtime survives saturation: new observations still actuate.
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        2,
        vec![
            win(11, 0, 0, 640, 1080),
            win(12, 640, 0, 640, 1080),
            win(13, 1280, 0, 640, 1080),
        ],
        Some(13),
    ));
    flush(&fixture.handle);
    assert!(!drain_applied(&fixture).is_empty());
    shutdown_clean(&fixture);
}

#[test]
fn input_matching_forwards_only_exact_keydown() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    // No applies rendezvous is needed here: the ignored-input assertions
    // below read engine-side counters only, which worker applies never
    // touch.
    let baseline = fixture.handle.diagnostics();

    // Key-up, wrong modifier, extra modifier, and wrong key are all ignored
    // with no core contact.
    let extra_modifier = flags(ProbeModifier::Ctrl) | flags(ProbeModifier::Shift);
    send_input_and_flush(
        &fixture,
        &[
            key(TapKeyKind::KeyUp, flags(ProbeModifier::Ctrl), 0x04),
            key(TapKeyKind::KeyDown, flags(ProbeModifier::Meta), 0x04),
            key(TapKeyKind::KeyDown, extra_modifier, 0x04),
            key(TapKeyKind::KeyDown, flags(ProbeModifier::Ctrl), 0x09),
            key(TapKeyKind::Other, flags(ProbeModifier::Ctrl), 0x04),
        ],
        baseline.input_forwarded,
        baseline.input_ignored + 5,
    );
    let after_ignored = fixture.handle.diagnostics();
    assert_eq!(
        after_ignored.input_ignored,
        baseline.input_ignored + 5,
        "inexact/non-down events are ignored"
    );
    assert_eq!(
        after_ignored.input_forwarded, baseline.input_forwarded,
        "nothing but exact key-down forwards"
    );
    assert_eq!(
        after_ignored.core_replies, baseline.core_replies,
        "ignored input never reaches the core"
    );

    // Exact focus chord: the core resolves the step and emits a focus
    // request (dispatched, never verified).
    send_input_and_flush(
        &fixture,
        &[focus_key()],
        baseline.input_forwarded + 1,
        baseline.input_ignored + 5,
    );
    let after_focus = fixture.handle.diagnostics();
    assert_eq!(after_focus.input_forwarded, baseline.input_forwarded + 1);
    let focus_replies: u64 = after_focus
        .core_replies
        .iter()
        .filter(|(k, _)| k.starts_with("focus-"))
        .map(|(_, v)| v)
        .sum();
    assert_eq!(
        focus_replies, 1,
        "focus step reaches the core: {:?}",
        after_focus.core_replies
    );
    let requests = fixture.handle.drain_focus_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].window, "1001_12");
    assert!(requests[0].dispatched);
    assert!(!requests[0].verified);
    // No applies rendezvous is needed either: the focus assertions above
    // read the engine-side request queue only.

    // Exact move chord: the core resolves the move and re-plans geometry.
    let before_move = fixture.handle.last_desired_geometry();
    send_input_and_flush(
        &fixture,
        &[move_key()],
        baseline.input_forwarded + 2,
        baseline.input_ignored + 5,
    );
    let after_move = fixture.handle.diagnostics();
    let move_replies: u64 = after_move
        .core_replies
        .iter()
        .filter(|(k, _)| k.starts_with("move-"))
        .map(|(_, v)| v)
        .sum();
    assert_eq!(
        move_replies, 1,
        "move step reaches the core: {:?}",
        after_move.core_replies
    );
    let moved = fixture.handle.last_desired_geometry();
    assert!(!moved.is_empty());
    let _ = before_move;
    // Move actuation reaches the worker with the re-planned rects:
    // rendezvous on convergence with the move plan. Stale applies from
    // earlier plans share the window keys, so only the move re-plan's rects
    // converge (latest-wins coalescing is covered either way).
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut seen: BTreeMap<String, (i32, i32, i32, i32)> = BTreeMap::new();
    loop {
        while let Ok(record) = fixture.applied_rx.try_recv() {
            seen.insert(record.window.clone(), record.rect);
        }
        let converged = moved
            .iter()
            .all(|f| seen.get(&f.window) == Some(&(f.x, f.y, f.w, f.h)));
        if converged {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("move re-plan never actuated: moved={moved:?} seen={seen:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    shutdown_clean(&fixture);
}

#[test]
fn pid_reuse_bumps_generation_and_refuses_overlap() {
    let fixture = start(test_config());
    let gen1 = fixture.handle.launch_app(APP_A).expect("first launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        gen1,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    collect_for_pids(&fixture, &[(APP_A, vec![1])]);
    assert_eq!(
        fixture.handle.terminate_app(APP_A),
        TerminateOutcome::Terminated { generation: gen1 }
    );

    // Clean replacement gets a fresh monotonic generation with its own
    // sequence space: seq 1 is accepted, not stale.
    let gen2 = relaunch(&fixture.handle, APP_A);
    assert!(gen2 > gen1, "generations must be monotonic");
    fixture.handle.submit_observation(obs(
        APP_A,
        gen2,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    assert_eq!(
        fixture.handle.diagnostics().stale_dropped,
        0,
        "fresh generation restarts the sequence space"
    );
    let applied = collect_for_pids(&fixture, &[(APP_A, vec![1])]);
    assert!(
        applied.iter().any(|r| r.generation == gen2),
        "relaunched apply must carry the new generation: {applied:?}"
    );

    // Overlapping replacement while the previous generation still executes
    // is refused.
    drain_entered(&fixture);
    let gate = gate_app(&fixture, APP_B);
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    fixture.handle.submit_observation(obs(
        APP_B,
        gen_b,
        1,
        vec![win(21, 0, 0, 1920, 1080)],
        Some(21),
    ));
    flush(&fixture.handle);
    let _ = wait_entered(&fixture, APP_B);
    assert_eq!(
        fixture.handle.terminate_app(APP_B),
        TerminateOutcome::Terminated { generation: 3 }
    );
    let err = fixture
        .handle
        .launch_app(APP_B)
        .expect_err("overlap must be refused");
    assert!(
        matches!(err, StartupError::PreviousGenerationRunning { .. }),
        "expected overlap refusal, got {err:?}"
    );
    release_gate(&gate);
    flush(&fixture.handle);
    // The exited generation no longer blocks replacement.
    let gen_b2 = relaunch(&fixture.handle, APP_B);
    assert!(gen_b2 > 3);
    shutdown_clean(&fixture);
}

#[test]
fn terminate_keeps_survivors_operational() {
    let fixture = start(test_config());
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch A");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    // Globally consistent rects: the skeleton domain is shared, so windows
    // from different apps must tile it without overlap.
    fixture.handle.submit_observation(obs(
        APP_A,
        gen_a,
        1,
        vec![win(11, 0, 0, 480, 1080), win(12, 480, 0, 480, 1080)],
        Some(11),
    ));
    fixture.handle.submit_observation(obs(
        APP_B,
        gen_b,
        1,
        vec![win(21, 960, 0, 480, 1080), win(22, 1440, 0, 480, 1080)],
        Some(21),
    ));
    flush(&fixture.handle);
    assert_eq!(fixture.handle.last_desired_geometry().len(), 4);
    // Rendezvous with the whole first plan (both apps) so nothing
    // still in-flight is later mistaken for post-terminate work. Every
    // first-plan command converges: both apps actuate the 4-window tiling.
    // (Latest-wins may coalesce per-app takes, so this waits on rects, not
    // command counts.)
    let desired_before = fixture.handle.last_desired_geometry();
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut seen: BTreeMap<String, (i32, i32, i32, i32)> = BTreeMap::new();
    loop {
        while let Ok(record) = fixture.applied_rx.try_recv() {
            seen.insert(record.window.clone(), record.rect);
        }
        let converged = !desired_before.is_empty()
            && desired_before
                .iter()
                .all(|f| seen.get(&f.window) == Some(&(f.x, f.y, f.w, f.h)));
        if converged {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("first plan never actuated: desired={desired_before:?} seen={seen:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Terminating A removes exactly A's windows from the core plan.
    let TerminateOutcome::Terminated { .. } = fixture.handle.terminate_app(APP_A) else {
        panic!("terminate A");
    };
    flush(&fixture.handle);
    let desired = fixture.handle.last_desired_geometry();
    let mut names: Vec<&str> = desired.iter().map(|f| f.window.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["1002_21", "1002_22"],
        "only survivors remain: {desired:?}"
    );

    // B actuates the survivor plan: rendezvous on convergence of B's
    // applied rects with the desired survivor geometry. Stale pre-terminate
    // applies (already-in-flight work) are observed but never mistaken for
    // the post-stop plan: only convergence with the post-terminate desired
    // set passes.
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut seen: BTreeMap<String, (i32, i32, i32, i32)> = BTreeMap::new();
    loop {
        while let Ok(record) = fixture.applied_rx.try_recv() {
            if record.pid == APP_B {
                seen.insert(record.window, record.rect);
            }
        }
        let converged = desired
            .iter()
            .all(|f| seen.get(&f.window) == Some(&(f.x, f.y, f.w, f.h)));
        if converged {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "survivor never actuated the post-terminate plan: desired={desired:?} seen={seen:?}"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    shutdown_clean(&fixture);
}

#[test]
fn backend_timeout_completion_is_honest_not_truth() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.ax.timeout_once.lock().expect("mutex").insert(APP_A);
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    // The timeout completion arrives as a result, and the engine logs it
    // without treating it as observed truth: the desired plan is unchanged
    // and no frame was applied.
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.completions.get("timeout").copied().unwrap_or(0) >= 1,
        "timeout completion",
    );
    assert_eq!(
        diagnostics.completions.get("timeout").copied().unwrap_or(0),
        1,
        "timeout completion must be recorded honestly: {diagnostics:?}"
    );
    assert!(
        fixture.applied_rx.try_recv().is_err(),
        "a timeout must not fabricate an applied frame"
    );
    assert_eq!(fixture.handle.last_desired_geometry().len(), 1);
    shutdown_clean(&fixture);
}

#[test]
fn structured_log_reconstructs_lifecycle() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let _ = wait_applied(&fixture);
    // The applied completion is processed asynchronously; rendezvous with
    // the engine-logged `ax-result/applied` before terminating so the log
    // shows the full lifecycle. Neither backend dispatch truth
    // (`wait_applied`) nor the `completions["applied"]` counter proves it:
    // the counter increments even for late results the engine then logs
    // as `terminated-invalidated`, so only the ring entry itself orders
    // the completion ahead of terminate.
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "ax-result" && e.outcome == "applied")
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "engine never logged ax-result/applied: {:?}",
                fixture.handle.log_snapshot()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let TerminateOutcome::Terminated { .. } = fixture.handle.terminate_app(APP_A) else {
        panic!("terminate");
    };
    flush(&fixture.handle);

    let log = fixture.handle.log_snapshot();
    let kinds: Vec<(&str, &str)> = log.iter().map(|e| (e.kind, e.outcome)).collect();
    for expected in [
        ("lifecycle", "launched"),
        ("observation", "applied"),
        ("ax-result", "applied"),
        ("lifecycle", "terminated"),
    ] {
        assert!(
            kinds.contains(&expected),
            "log must reconstruct {expected:?}: {kinds:?}"
        );
    }
    assert!(
        log.iter()
            .filter(|e| e.pid == APP_A)
            .all(|e| e.generation == generation),
        "entries carry the app generation"
    );
    // No user content in the log: kinds/outcomes are static tokens and
    // reference IDs are numeric.
    shutdown_clean(&fixture);
    let _ = InputDirection::Left;
}

#[test]
fn stale_generation_observation_rejected_after_relaunch() {
    let fixture = start(test_config());
    let gen1 = fixture.handle.launch_app(APP_A).expect("first launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        gen1,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    // Rendezvous with the first lifetime's apply so it cannot leak into
    // the post-relaunch oracle.
    collect_for_pids(&fixture, &[(APP_A, vec![1])]);
    let plan_before = fixture.handle.last_desired_geometry();
    assert_eq!(plan_before.len(), 1);

    assert_eq!(
        fixture.handle.terminate_app(APP_A),
        TerminateOutcome::Terminated { generation: gen1 }
    );
    flush(&fixture.handle);
    // Terminate reconciles the survivors: A is gone from the plan.
    assert!(fixture.handle.last_desired_geometry().is_empty());

    let gen2 = relaunch(&fixture.handle, APP_A);
    assert!(gen2 > gen1);
    // A late observation from the old lifetime is refused, not stamped with
    // the new generation: the plan is untouched.
    assert_eq!(
        fixture
            .handle
            .submit_observation(obs(APP_A, gen1, 2, vec![win(11, 0, 0, 1, 1)], Some(11))),
        SubmitOutcome::StaleGeneration
    );
    flush(&fixture.handle);
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics.stale_dropped >= 1,
        "stale lifetime must be rejected: {diagnostics:?}"
    );
    assert!(
        fixture.handle.last_desired_geometry().is_empty(),
        "stale observation must not move the new lifetime"
    );

    // The new lifetime has its own sequence space: seq 1 is fresh.
    assert_eq!(
        fixture.handle.submit_observation(obs(
            APP_A,
            gen2,
            1,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11)
        )),
        SubmitOutcome::Accepted
    );
    flush(&fixture.handle);
    assert_eq!(
        fixture.handle.diagnostics().stale_dropped,
        diagnostics.stale_dropped
    );
    assert_eq!(fixture.handle.last_desired_geometry().len(), 1);
    // Fresh lifetime, fresh sequence space: the apply carries gen2.
    let applied = collect_for_pids(&fixture, &[(APP_A, vec![1])]);
    assert!(
        applied.iter().any(|r| r.generation == gen2),
        "post-relaunch apply must carry the new generation: {applied:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn worker_not_ready_leaves_runtime_usable_and_late_launch_is_stale() {
    let config = RuntimeConfig {
        worker_ready_timeout: Duration::from_millis(100),
        ..test_config()
    };
    let fixture = start(config);
    // Block backend creation so the sync launch times out waiting for it.
    let create_gate = gate_create_app(&fixture, APP_A);
    assert_eq!(
        fixture.handle.launch_app(APP_A),
        Err(StartupError::WorkerNotReady)
    );
    // The timed-out launch removes its entry without leaking it, and the
    // runtime stays usable: admission open, engine wakes.
    assert_eq!(fixture.handle.app_generation(APP_A), None);
    assert_eq!(
        fixture.handle.worker_ready(APP_A),
        WorkerReadiness::UnknownPid
    );
    assert!(fixture.handle.is_running());
    assert!(fixture.handle.flush(FLUSH_TIMEOUT));

    // Releasing lets the old worker finish creation and report its Launch —
    // but its entry is gone, so the engine rejects the late lifecycle as
    // stale instead of resurrecting old-lifetime state.
    release_gate(&create_gate);
    wait_diagnostics(
        &fixture.handle,
        |d| d.stale_dropped >= 1,
        "stale late launch",
    );
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "launch" && e.outcome == "stale-rejected"),
        "late launch from the timed-out generation must be stale-rejected"
    );
    // The exited generation no longer blocks replacement.
    let gen2 = relaunch(&fixture.handle, APP_A);
    assert!(gen2 >= 2);
    fixture.handle.submit_observation(obs(
        APP_A,
        gen2,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let applied = collect_for_pids(&fixture, &[(APP_A, vec![1])]);
    assert!(
        applied.iter().any(|r| r.generation == gen2),
        "replacement apply must carry the new generation: {applied:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn async_launch_reports_readiness_without_blocking() {
    let fixture = start(test_config());
    // The nonblocking path admits immediately and reports readiness once
    // the worker creates its backend inside the owning thread.
    let generation = fixture
        .handle
        .launch_app_async(APP_A)
        .expect("async admits");
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        match fixture.handle.worker_ready(APP_A) {
            WorkerReadiness::Ready => break,
            WorkerReadiness::Pending => {
                assert!(std::time::Instant::now() < deadline, "backend never ready");
                std::thread::sleep(Duration::from_millis(5));
            }
            other => panic!("unexpected readiness: {other:?}"),
        }
    }
    // The worker reported its own Launch before marking ready, so the
    // engine already knows this generation: observations actuate.
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let applied = wait_applied(&fixture);
    assert_eq!((applied.pid, applied.generation), (APP_A, generation));
    assert_eq!(
        fixture.handle.worker_ready(9999),
        WorkerReadiness::UnknownPid
    );
    shutdown_clean(&fixture);
}

#[test]
fn input_eos_is_observed_not_spun() {
    let fixture = start(test_config());
    fixture.handle.launch_app(APP_A).expect("launch");
    // Disconnecting the source is unexpected EOS: the input thread exits
    // (counted and logged) instead of busy-spinning on `None`. The death
    // is observable: admission closes and the terminal shutdown is unclean,
    // never a false clean.
    drop(fixture.input_tx);
    wait_diagnostics(
        &fixture.handle,
        |d| d.unexpected_eos >= 1,
        "unexpected input EOS",
    );
    assert!(
        !fixture.handle.is_running(),
        "dead input owner must read as stopped"
    );
    // The engine is unaffected and still wakes for the barrier, but the
    // shutdown reports the EOS honestly.
    assert!(fixture.handle.flush(FLUSH_TIMEOUT));
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Unclean, "{report:?}");
}

#[test]
fn focus_plans_never_advance_observed_truth() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![
            win(11, 0, 0, 640, 1080),
            win(12, 640, 0, 640, 1080),
            win(13, 1280, 0, 640, 1080),
        ],
        Some(11),
    ));
    flush(&fixture.handle);
    drain_applied(&fixture);

    // Two focus steps with no new observation: both resolve from observed
    // focus 11, so both dispatch 1001_12. A planned (dispatched but
    // unobserved) focus must not become the next step's subject.
    send_input_and_flush(&fixture, &[focus_key(), focus_key()], 2, 0);
    let requests = fixture.handle.drain_focus_requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    for request in &requests {
        assert_eq!(request.window, "1001_12", "{requests:?}");
        assert_eq!(request.generation, generation);
        assert!(request.dispatched);
        assert!(!request.verified);
    }
    assert_ne!(requests[0].request_id, requests[1].request_id);
    shutdown_clean(&fixture);
}

#[test]
fn terminated_focus_is_purged_and_drain_stays_fresh() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    drain_applied(&fixture);
    // Dispatch one focus request and leave it queued.
    send_input_and_flush(&fixture, &[focus_key()], 1, 0);

    // Terminating purges the queued request: dispatch never outlives the
    // app lifetime it was planned for.
    assert_eq!(
        fixture.handle.terminate_app(APP_A),
        TerminateOutcome::Terminated { generation }
    );
    assert!(fixture.handle.drain_focus_requests().is_empty());
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics.terminated_invalidated >= 1,
        "purged focus must be invalidated: {diagnostics:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn concurrent_terminate_and_shutdown_with_queued_focus_cannot_deadlock() {
    use std::sync::Barrier;
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    drain_applied(&fixture);
    // Leave a focus request queued so both sides contend on the focus queue:
    // terminate purges it while shutdown invalidates it.
    send_input_and_flush(&fixture, &[focus_key()], 1, 0);
    assert_eq!(fixture.handle.drain_focus_requests().len(), 1);
    send_input_and_flush(&fixture, &[focus_key()], 2, 0);

    // Race terminate against shutdown from two threads behind a barrier.
    // Completion travels over channels with a deadline: a lock-order
    // regression fails the test instead of hanging the suite.
    let barrier = Arc::new(Barrier::new(2));
    let (done_tx, done_rx) = mpsc::channel::<&'static str>();
    std::thread::scope(|scope| {
        let handle = &fixture.handle;
        let barrier_terminate = Arc::clone(&barrier);
        let done_terminate = done_tx.clone();
        scope.spawn(move || {
            barrier_terminate.wait();
            let _ = handle.terminate_app(APP_A);
            let _ = done_terminate.send("terminate");
        });
        let barrier_shutdown = Arc::clone(&barrier);
        let done_shutdown = done_tx.clone();
        scope.spawn(move || {
            barrier_shutdown.wait();
            let _ = handle.shutdown(Duration::from_secs(5));
            let _ = done_shutdown.send("shutdown");
        });
        drop(done_tx);
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut finished = Vec::new();
        while finished.len() < 2 {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match done_rx.recv_timeout(remaining) {
                Ok(side) => finished.push(side),
                Err(_) => panic!("terminate/shutdown deadlocked: finished={finished:?}"),
            }
        }
        assert!(finished.contains(&"terminate") && finished.contains(&"shutdown"));
    });
}

// -- Backend-generated observation handoff -----------------------------------
// These tests prove the thread-local AX observation path: the backend (on
// its owner thread) emits typed observations that reach the actual core
// with worker-stamped generation/sequence, without the main thread ever
// calling `submit_observation`.

/// Wait until the engine's desired geometry covers exactly `names`, then
/// return it. Deadline-bounded; the rendezvous is exact, not timing luck.
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

#[test]
fn backend_observations_reach_core_without_main_submit() {
    let fixture = start(test_config());
    // Startup snapshot content: the worker publishes it on its own thread.
    configure_snapshot(&fixture, APP_A, vec![win(11, 0, 0, 960, 1080)]);
    let obs_tx = autonomous_app(&fixture, APP_A);
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    // No `submit_observation` call anywhere in this test: the startup
    // observation alone must drive the first core plan.
    wait_desired_names(&fixture, &["1001_11"]);
    let applied = wait_applied(&fixture);
    assert_eq!((applied.pid, applied.generation), (APP_A, generation));

    // A backend-streamed change (same channel a native notification pump
    // would feed) reaches the core and actuates, still with no main submit.
    let changed = vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)];
    configure_snapshot(&fixture, APP_A, changed.clone());
    obs_tx
        .send(Ok(backend_snapshot(changed, Some(11))))
        .expect("backend send");
    wait_desired_names(&fixture, &["1001_11", "1001_12"]);

    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.observations_published >= 2,
        "backend-published observations",
    );
    assert!(
        diagnostics.observations_published >= 2,
        "both backend observations must publish: {diagnostics:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn observation_failure_preserves_retained_state() {
    let fixture = start(test_config());
    configure_snapshot(&fixture, APP_A, vec![win(11, 0, 0, 1920, 1080)]);
    let obs_tx = autonomous_app(&fixture, APP_A);
    fixture.handle.launch_app(APP_A).expect("launch");
    wait_desired_names(&fixture, &["1001_11"]);
    let plan_before = fixture.handle.last_desired_geometry();
    let published_before = fixture.handle.diagnostics().observations_published;

    // A backend observation failure must not clobber retained state: the
    // plan is untouched and the failure is counted honestly.
    obs_tx
        .send(Err("fake hung app".to_owned()))
        .expect("backend send");
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.observation_failures >= 1,
        "observation failure",
    );
    flush(&fixture.handle);
    assert_eq!(
        fixture.handle.last_desired_geometry(),
        plan_before,
        "failed observation must preserve the retained plan"
    );
    assert_eq!(
        fixture.handle.diagnostics().observations_published,
        published_before,
        "failures never publish"
    );
    assert!(diagnostics.observation_failures >= 1, "{diagnostics:?}");
    shutdown_clean(&fixture);
}

#[test]
fn startup_change_and_readback_converge_without_replay() {
    let fixture = start(test_config());
    let v1 = vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)];
    configure_snapshot(&fixture, APP_A, v1.clone());
    let obs_tx = autonomous_app(&fixture, APP_A);
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    // Startup handoff: worker-published, core-planned, worker-actuated.
    wait_desired_names(&fixture, &["1001_11", "1001_12"]);
    let first = wait_applied(&fixture);
    assert_eq!(first.generation, generation);

    // Change handoff: backend streams v2 (snapshot store updated alongside,
    // as live native state would be), the core re-plans, the worker
    // actuates the re-planned rects.
    let v2 = vec![win(11, 0, 0, 1920, 1080)];
    configure_snapshot(&fixture, APP_A, v2.clone());
    obs_tx
        .send(Ok(backend_snapshot(v2, Some(11))))
        .expect("backend send");
    wait_desired_names(&fixture, &["1001_11"]);
    flush(&fixture.handle);

    // Readback handoff: post-apply re-reads match the last published state,
    // so nothing republishes — a native clamp/refusal never replays
    // endlessly. Drain applies, settle, and prove the publish count rests
    // at exactly startup + change.
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.observations_published >= 2,
        "change publication",
    );
    assert_eq!(
        diagnostics.observations_published, 2,
        "readbacks must skip unchanged state: {diagnostics:?}"
    );
    drain_applied(&fixture);
    std::thread::sleep(Duration::from_millis(150));
    flush(&fixture.handle);
    assert_eq!(
        fixture.handle.diagnostics().observations_published,
        2,
        "no replay after convergence: {:?}",
        fixture.handle.diagnostics()
    );
    shutdown_clean(&fixture);
}

#[test]
fn hung_observation_pump_cannot_stall_others() {
    let fixture = start(test_config());
    // A is gated inside its observation pump; B streams freely. A's
    // snapshot stays unconfigured until release, so only B drives the plan
    // while A is hung (its startup honestly fails instead of publishing
    // an unconfigured empty list as truth).
    let poll_gate = gate_poll_app(&fixture, APP_A);
    configure_snapshot(&fixture, APP_B, vec![win(21, 0, 0, 960, 1080)]);
    let obs_tx_b = autonomous_app(&fixture, APP_B);
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    let _gen_a = fixture.handle.launch_app(APP_A).expect("launch A");

    // While A sits in its pump, B's backend observation plus input both
    // progress (the rendezvous below proves the engine is not stalled).
    // The flush first establishes B's observed focus so the input step has
    // a subject; input sent earlier would race focus establishment.
    obs_tx_b
        .send(Ok(backend_snapshot(
            vec![win(21, 0, 0, 480, 1080), win(22, 480, 0, 480, 1080)],
            Some(21),
        )))
        .expect("backend send");
    configure_snapshot(
        &fixture,
        APP_B,
        vec![win(21, 0, 0, 480, 1080), win(22, 480, 0, 480, 1080)],
    );
    flush(&fixture.handle);
    send_input_and_flush(&fixture, &[focus_key()], 1, 0);
    wait_desired_names(&fixture, &["1002_21", "1002_22"]);

    let diagnostics = fixture.handle.diagnostics();
    assert!(diagnostics.input_forwarded >= 1, "input must progress");
    assert_eq!(
        fixture.handle.app_generation(APP_B),
        Some(gen_b),
        "B keeps its lifetime while A is hung"
    );

    // Release A: its pump drains, its (now configured) startup publishes,
    // and the runtime shuts down clean (no panic, no retained handle).
    release_gate(&poll_gate);
    configure_snapshot(&fixture, APP_A, vec![win(11, 0, 0, 960, 1080)]);
    wait_desired_names(&fixture, &["1001_11", "1002_21", "1002_22"]);
    shutdown_clean(&fixture);
}

// -- Structured normal observability + honest teardown ----------------------
// These tests cover the normal-log sink seam and the no-false-clean rules:
// failed starts stay reaped, teardown failures and EOS poison the outcome,
// and a slow sink never stalls the engine.

struct CollectSink {
    events: Mutex<Vec<LogEvent>>,
}

impl LogSink for CollectSink {
    fn write(&self, event: &LogEvent) {
        self.events.lock().expect("mutex").push(event.clone());
    }
}

fn collected_kinds(sink: &CollectSink) -> Vec<(String, String)> {
    sink.events
        .lock()
        .expect("mutex")
        .iter()
        .map(|e| (e.kind.to_owned(), e.outcome.to_owned()))
        .collect()
}

/// Rendezvous with the async sink: poll until an event matches or the
/// deadline expires. Diagnostics-only waits race the logger thread, so sink
/// assertions must wait on the sink itself, never on counters alone.
fn wait_sink(sink: &CollectSink, wanted: impl Fn(&LogEvent) -> bool, what: &str) {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if sink.events.lock().expect("mutex").iter().any(&wanted) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "sink never showed {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// True when `wanted` appears as an ordered subsequence of `got`.
fn contains_ordered(got: &[(String, String)], wanted: &[(&str, &str)]) -> bool {
    let mut index = 0;
    for entry in got {
        if index < wanted.len() && entry.0 == wanted[index].0 && entry.1 == wanted[index].1 {
            index += 1;
        }
    }
    index == wanted.len()
}

fn start_with_collect(
    config: RuntimeConfig,
    sink: Arc<CollectSink>,
) -> (
    RuntimeHandle,
    Arc<FakeAxInner>,
    mpsc::Receiver<AppliedRecord>,
    mpsc::Receiver<EnteredRecord>,
    mpsc::Sender<RawKeyEvent>,
) {
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
    let handle = Runtime::start_with_sink(
        config,
        Arc::new(FakeAxFactory {
            inner: Arc::clone(&ax),
        }),
        input,
        sink.clone(),
    )
    .expect("runtime starts");
    (handle, ax, applied_rx, entered_rx, input_tx)
}

#[test]
fn structured_sink_reconstructs_start_to_stop() {
    let sink = Arc::new(CollectSink {
        events: Mutex::new(Vec::new()),
    });
    let (handle, _ax, applied_rx, _entered_rx, _input_tx) =
        start_with_collect(test_config(), Arc::clone(&sink));
    let generation = handle.launch_app(APP_A).expect("launch");
    handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    assert!(handle.flush(FLUSH_TIMEOUT));
    let applied = applied_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("applied frame arrives");
    assert_eq!(applied.pid, APP_A);
    // The async completion lands engine-side; rendezvous before stopping so
    // the sink sees the full chain.
    wait_diagnostics(
        &handle,
        |d| d.completions.get("applied").copied().unwrap_or(0) >= 1,
        "applied completion",
    );
    // Completion detail is preserved, not dropped: elapsed millis ride the
    // count and the outcome stays a static token. Rendezvous on the sink
    // itself: the diagnostics wait above races the logger thread.
    wait_sink(
        &sink,
        |e| e.kind == "ax-result" && e.outcome == "applied",
        "applied completion in the sink",
    );
    let report = handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
    // The logger drains under shutdown; poll for the terminal entries with
    // a deadline instead of assuming write timing.
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let kinds = loop {
        let kinds = collected_kinds(&sink);
        if contains_ordered(&kinds, &[("shutdown", "clean"), ("shutdown", "reported")])
            || std::time::Instant::now() >= deadline
        {
            break kinds;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    for wanted in [
        ("runtime", "started"),
        ("lifecycle", "launched"),
        ("observation", "applied"),
        ("ax-result", "applied"),
        ("shutdown", "stop-signalled"),
    ] {
        assert!(
            contains_ordered(&kinds, &[wanted]),
            "sink must see {wanted:?}: {kinds:?}"
        );
    }
    // The core plan reply is `tiled` or `projection` depending on layout;
    // either proves the core step ran between observation and frame.
    assert!(
        contains_ordered(&kinds, &[("core-reply", "tiled")])
            || contains_ordered(&kinds, &[("core-reply", "projection")]),
        "sink must see the core reply: {kinds:?}"
    );
    assert!(
        contains_ordered(
            &kinds,
            &[
                ("runtime", "started"),
                ("lifecycle", "launched"),
                ("observation", "applied"),
            ]
        ) && contains_ordered(
            &kinds,
            &[
                ("frame", "emitted"),
                ("ax-result", "applied"),
                ("shutdown", "stop-signalled"),
            ]
        ),
        "sink must order start->launch->obs->core->frame->completion->stop: {kinds:?}"
    );
    // No user content in the sink: every kind/outcome is a static token and
    // every event carries numeric identity plus a monotonic timestamp.
    let events = sink.events.lock().expect("mutex").clone();
    assert!(!events.is_empty());
    let mut last_seq = 0;
    for event in &events {
        assert!(event.seq > last_seq, "correlation must be monotonic");
        last_seq = event.seq;
        assert!(!event.kind.is_empty() && !event.outcome.is_empty());
    }
}

#[test]
fn failed_completion_reason_is_sanitized_not_leaked() {
    let sink = Arc::new(CollectSink {
        events: Mutex::new(Vec::new()),
    });
    let (handle, ax, applied_rx, _entered_rx, _input_tx) =
        start_with_collect(test_config(), Arc::clone(&sink));
    let generation = handle.launch_app(APP_A).expect("launch");
    // A refused apply reports dispatch truth with a sanitized static token;
    // the raw backend string never reaches the sink.
    ax.timeout_once.lock().expect("mutex").insert(APP_A);
    handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    assert!(handle.flush(FLUSH_TIMEOUT));
    wait_diagnostics(
        &handle,
        |d| d.completions.get("timeout").copied().unwrap_or(0) >= 1,
        "timeout completion",
    );
    assert!(
        applied_rx.try_recv().is_err(),
        "a timeout must not fabricate an applied frame"
    );
    // Same sink race as above: rendezvous on the sink event itself.
    wait_sink(
        &sink,
        |e| e.kind == "ax-result" && e.outcome == "timeout" && e.count == 2000,
        "timeout completion with elapsed in the sink",
    );
    let report = handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn slow_sink_never_stalls_engine_or_input() {
    // A sink that blocks until released: the bounded queue must saturate
    // (counted drops) while the engine and input keep progressing.
    struct SlowSink {
        gate: Arc<Gate>,
        events: Mutex<Vec<LogEvent>>,
    }
    impl LogSink for SlowSink {
        fn write(&self, event: &LogEvent) {
            let released = self.gate.released.lock().expect("mutex");
            let mut released = released;
            while !*released {
                released = self.gate.cond.wait(released).expect("mutex");
            }
            self.events.lock().expect("mutex").push(event.clone());
        }
    }
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        cond: Condvar::new(),
    });
    let sink = Arc::new(SlowSink {
        gate: Arc::clone(&gate),
        events: Mutex::new(Vec::new()),
    });
    let config = RuntimeConfig {
        log_queue_depth: 4,
        engine_queue_depth: 128,
        ..test_config()
    };
    let (applied_tx, applied_rx) = mpsc::channel::<AppliedRecord>();
    let (entered_tx, _entered_rx) = mpsc::channel::<EnteredRecord>();
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
    let handle = Runtime::start_with_sink(
        config,
        Arc::new(FakeAxFactory { inner: ax }),
        input,
        sink.clone(),
    )
    .expect("runtime starts");
    let generation = handle.launch_app(APP_A).expect("launch");
    // Flood observations while the sink is blocked: the engine must still
    // drain them (barrier proves it) and the saturated log queue must drop
    // counted instead of stalling anyone.
    for seq in 1..=20u64 {
        handle.submit_observation(obs(
            APP_A,
            generation,
            seq,
            vec![win(11, 0, 0, 1920, 1080)],
            Some(11),
        ));
    }
    input_tx.send(focus_key()).expect("input send");
    assert!(handle.flush(FLUSH_TIMEOUT), "engine must not stall on sink");
    wait_diagnostics(
        &handle,
        |d| d.input_forwarded >= 1,
        "input classification behind a blocked sink",
    );
    assert!(
        applied_rx.recv_timeout(WAIT_TIMEOUT).is_ok(),
        "frames must actuate behind a blocked sink"
    );
    let diagnostics = handle.diagnostics();
    assert!(
        diagnostics.logger_dropped >= 1,
        "saturation must drop counted: {diagnostics:?}"
    );
    // Bounded shutdown while the logger is still blocked: the logger handle
    // is retained (dirty, never a false clean), never an unbounded wait.
    let start = std::time::Instant::now();
    let report = handle.shutdown(Duration::from_millis(300));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "shutdown must respect the deadline: {:?}",
        start.elapsed()
    );
    assert_eq!(report.outcome, ShutdownOutcome::DirtyTimeout, "{report:?}");
    assert!(report.retained >= 1, "slow logger must be retained");
    // Release the sink: the retained logger drains and reaps to empty.
    release_gate(&gate);
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let reap = loop {
        let reap = handle.reap();
        if reap.remaining == 0 || std::time::Instant::now() >= deadline {
            break reap;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(reap.remaining, 0, "released logger must reap: {reap:?}");
}

#[test]
fn teardown_failure_is_unclean_not_clean() {
    let fixture = start(test_config());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    fixture
        .ax
        .shutdown_fail
        .lock()
        .expect("mutex")
        .insert(APP_A);
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Unclean, "{report:?}");
    // Single accounting: the live teardown error is logged, and the join
    // adds the persistent evidence exactly once — never both.
    assert_eq!(
        report.unclean, 1,
        "teardown failure counts once: {report:?}"
    );
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics.worker_unclean_observed, 1,
        "teardown evidence persists exactly once: {diagnostics:?}"
    );
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "observer" && e.outcome == "teardown-failed"),
        "teardown failure must be logged"
    );
    // The terminal report replays exactly: no false clean on repeat.
    let again = fixture.handle.shutdown(Duration::from_secs(1));
    assert_eq!(again, report, "terminal report must replay exactly");
}

#[test]
fn reaped_worker_panic_still_poisons_shutdown() {
    let fixture = start(test_config());
    fixture.handle.launch_app(APP_A).expect("launch");
    fixture.ax.panic_pids.lock().expect("mutex").insert(APP_A);
    fixture
        .handle
        .submit_observation(obs(APP_A, 1, 1, vec![win(11, 0, 0, 1920, 1080)], Some(11)));
    flush(&fixture.handle);
    let _ = wait_entered(&fixture, APP_A);
    // The panicked generation exited: a replacement launch reaps the panic
    // (bounded retry on the honest overlap refusal) and the evidence
    // persists past the reap.
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let gen2 = loop {
        match fixture.handle.launch_app_async(APP_A) {
            Ok(generation) => break generation,
            Err(StartupError::PreviousGenerationRunning { .. }) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "panicked worker never exited"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(other) => panic!("unexpected launch outcome: {other:?}"),
        }
    };
    assert!(gen2 > 1);
    assert!(
        fixture.handle.diagnostics().worker_panics_observed >= 1,
        "reaped panic must persist"
    );
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Panic, "{report:?}");
}

#[test]
fn blocked_input_startup_retains_handle_for_reap() {
    // The input factory blocks inside creation; the bounded startup timeout
    // fails fast with a dirty report while keeping the handle reachable.
    let (applied_tx, _applied_rx) = mpsc::channel::<AppliedRecord>();
    let (entered_tx, _entered_rx) = mpsc::channel::<EnteredRecord>();
    let (_input_tx, input_rx) = mpsc::channel::<RawKeyEvent>();
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
    let create_gate = Arc::new(Gate {
        released: Mutex::new(false),
        cond: Condvar::new(),
    });
    let input = Arc::new(FakeInputFactory {
        rx: Mutex::new(Some(input_rx)),
        panic_on_next: AtomicBool::new(false),
        fail_create: AtomicBool::new(false),
        create_gate: Mutex::new(Some(Arc::clone(&create_gate))),
    });
    let config = RuntimeConfig {
        startup_ready_timeout: Duration::from_millis(150),
        ..test_config()
    };
    let start = std::time::Instant::now();
    let failure = match Runtime::start(config, Arc::new(FakeAxFactory { inner: ax }), input) {
        Err(failure) => failure,
        Ok(_) => panic!("blocked input must fail startup"),
    };
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "startup must respect its bound: {:?}",
        start.elapsed()
    );
    assert!(
        matches!(failure.error, StartupError::InputRefused(_)),
        "expected input not-ready, got {:?}",
        failure.error
    );
    assert_ne!(
        failure.report.outcome,
        ShutdownOutcome::Clean,
        "blocked start must not report clean: {:?}",
        failure.report
    );
    assert!(
        failure.report.retained >= 1,
        "blocked owner must be retained: {:?}",
        failure.report
    );
    assert!(!failure.handle.is_running());
    // Releasing lets the blocked creation finish and the owner exit; the
    // retained handle then reaps to empty with no false clean.
    release_gate(&create_gate);
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let reap = loop {
        let reap = failure.handle.reap();
        if reap.remaining == 0 || std::time::Instant::now() >= deadline {
            break reap;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(reap.remaining, 0, "released start must reap: {reap:?}");
    let again = failure.handle.shutdown(Duration::from_secs(1));
    assert_eq!(again, failure.report, "terminal report must replay exactly");
}

#[test]
fn failed_ax_status_reaches_sink_as_allowlisted_token() {
    let sink = Arc::new(CollectSink {
        events: Mutex::new(Vec::new()),
    });
    let (handle, ax, _applied_rx, _entered_rx, _input_tx) =
        start_with_collect(test_config(), Arc::clone(&sink));
    let generation = handle.launch_app(APP_A).expect("launch");
    // One refused apply carrying a specific OS status: the completion must
    // preserve the cause as an allowlisted token instead of collapsing it
    // into the coarse `failed-set` bucket.
    ax.fail_status_once.lock().expect("mutex").insert(APP_A);
    handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 1920, 1080)],
        Some(11),
    ));
    assert!(handle.flush(FLUSH_TIMEOUT));
    wait_diagnostics(
        &handle,
        |d| {
            d.completions
                .get("failed-ax-cannot-complete")
                .copied()
                .unwrap_or(0)
                >= 1
        },
        "allowlisted completion",
    );
    wait_sink(
        &sink,
        |e| e.kind == "ax-result" && e.outcome == "failed-ax-cannot-complete",
        "allowlisted completion in the sink",
    );
    // The raw backend string never reaches the sink: every outcome is a
    // static token with no status text, separators, or content.
    assert!(
        sink.events
            .lock()
            .expect("mutex")
            .iter()
            .all(|e| !e.outcome.contains(':') && !e.outcome.contains("kAX")),
        "no raw reason leaks: {:?}",
        sink.events.lock().expect("mutex")
    );
    let report = handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn observation_failure_outcome_is_sanitized_token() {
    let fixture = start(test_config());
    fixture.handle.launch_app(APP_A).expect("launch");
    // The startup snapshot is refused: the worker logs the sanitized token
    // for the refusal, never the raw backend string, and publishes nothing
    // (an error is never an empty observation).
    fixture
        .ax
        .snapshot_fail
        .lock()
        .expect("mutex")
        .insert(APP_A);
    wait_diagnostics(
        &fixture.handle,
        |d| d.observation_failures >= 1,
        "observation failure",
    );
    flush(&fixture.handle);
    let log = fixture.handle.log_snapshot();
    let outcomes: Vec<&str> = log
        .iter()
        .filter(|e| e.kind == "observation")
        .map(|e| e.outcome)
        .collect();
    assert!(!outcomes.is_empty(), "failure must be logged: {log:?}");
    assert!(
        outcomes.iter().all(|o| *o == "failed-snapshot"),
        "refusal sanitizes to its bucket: {outcomes:?}"
    );
    assert!(
        log.iter()
            .all(|e| !e.outcome.contains("fake") && !e.outcome.contains("1001")),
        "no backend strings leak into outcomes: {log:?}"
    );
    assert!(
        fixture.handle.last_desired_geometry().is_empty(),
        "a failed observation publishes nothing"
    );
    shutdown_clean(&fixture);
}

#[test]
fn dropped_handle_exits_workers_without_join() {
    let fixture = start(test_config());
    let gate = gate_app(&fixture, APP_A);
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    // Two windows so the taken batch holds two commands: the first is
    // provably inflight (entered rendezvous) while the second is still
    // taken-but-unstarted when the handle drops.
    fixture.handle.submit_observation(obs(
        APP_A,
        generation,
        1,
        vec![win(11, 0, 0, 960, 1080), win(12, 960, 0, 960, 1080)],
        Some(11),
    ));
    flush(&fixture.handle);
    let entered = wait_entered(&fixture, APP_A);
    assert_eq!(entered.cmd_id, 1, "first command is inflight");
    // Dropping the handle signals every app stop plus the frame slot (no
    // join): the worker must observe it without any shutdown call.
    let Fixture {
        handle, applied_rx, ..
    } = fixture;
    drop(handle);
    release_gate(&gate);
    // The inflight call may finish (taken is not inflight): exactly one
    // dispatch truth arrives...
    let first = applied_rx
        .recv_timeout(WAIT_TIMEOUT)
        .expect("inflight call finishes");
    assert_eq!(first.cmd_id, 1);
    // ...but the taken second command never starts. Before the Drop fix no
    // stop ever arrived: the second record would appear within milliseconds
    // and the worker would park forever retaining `Shared`.
    std::thread::sleep(Duration::from_millis(250));
    assert!(
        applied_rx.try_recv().is_err(),
        "a stopped worker starts no further commands"
    );
}

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
