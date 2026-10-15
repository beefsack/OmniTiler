//! Scoped-tile integration tests (offline, portable).
//!
//! Fakes drive the runtime through the native surface: revision-fenced
//! topology contexts, membership-carrying scoped snapshots, per-domain core
//! sessions over actual work areas, owner-dispatched focus, partial-write
//! completions, and the float-toggle input path. No live AX, no taps, no
//! Spaces, no desktop effects.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use tiler_macos::config::ProbeModifier;
use tiler_macos::ffi::{DisplayRect, NsDisplay, SlsDisplayRow};
use tiler_macos::input::cg_flag_bits;
use tiler_macos::input_match::TapKeyKind;
use tiler_macos::runtime::{
    AxCompletion, FocusCommand, FocusEvidence, FocusRoute, ObservedWindow, Runtime, RuntimeConfig,
    RuntimeHandle, ScopedBackendSnapshot, ScopedObservation, ScopedWindowMeta, ShutdownOutcome,
    SubmitOutcome, TopologyMode,
};
use tiler_macos::runtime_backend::{
    AxBackend, AxBackendFactory, InputSource, InputSourceFactory, RawKeyEvent,
};
use tiler_macos::topology;

const APP_A: u32 = 1001;
const APP_B: u32 = 1002;
const FLUSH: Duration = Duration::from_secs(5);
const WAIT: Duration = Duration::from_secs(5);

// -- Fakes -------------------------------------------------------------------

/// One applied frame: pid, cmd id, window id, rect, topology revision.
type AppliedRecord = (u32, u64, String, (i32, i32, i32, i32), u64);

struct FakeInner {
    applied_tx: mpsc::Sender<AppliedRecord>,
    /// Scripted scoped snapshots per pid (absent = read fails closed, so
    /// manually driven tests keep their own sequence space).
    scoped: Mutex<BTreeMap<u32, ScopedBackendSnapshot>>,
    /// Autonomous scoped inboxes, one per pid, installed before launch.
    poll_inboxes: Mutex<BTreeMap<u32, mpsc::Receiver<ScopedBackendSnapshot>>>,
    /// Latest-wins focus dispatches observed per pid.
    focus_tx: mpsc::Sender<FocusCommand>,
    gates: Mutex<BTreeMap<u32, Arc<Gate>>>,
    partial_once: Mutex<BTreeSet<u32>>,
    /// Every bound effect hook's frontmost leg, in bind order. The worker
    /// rebinds every multiplex iteration, so a frontmost update must show up
    /// here without any topology drift.
    hooks: Mutex<Vec<HookFrontmost>>,
}

struct Gate {
    released: Mutex<bool>,
    cond: std::sync::Condvar,
}

/// Bound hook frontmost leg: PID plus bundle (`None` = unknown session).
type HookFrontmost = Option<(u32, Option<String>)>;

struct FakeFactory {
    inner: Arc<FakeInner>,
}

impl AxBackendFactory for FakeFactory {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String> {
        Ok(Box::new(FakeBackend {
            pid,
            inner: Arc::clone(&self.inner),
            bound: None,
            poll_rx: self.inner.poll_inboxes.lock().expect("mutex").remove(&pid),
        }))
    }
}

struct FakeBackend {
    pid: u32,
    inner: Arc<FakeInner>,
    bound: Option<u64>,
    poll_rx: Option<mpsc::Receiver<ScopedBackendSnapshot>>,
}

impl AxBackend for FakeBackend {
    fn apply_frame(&mut self, cmd: &tiler_macos::runtime::FrameCommand) -> AxCompletion {
        // Clone the gate out of the map in its own statement: the map
        // guard must drop BEFORE the condvar wait below, or every other
        // worker deadlocks on the map lock while this one sleeps.
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
            .partial_once
            .lock()
            .expect("mutex")
            .remove(&self.pid)
        {
            return AxCompletion::PartialApplied {
                elapsed_ms: 4,
                size_first_applied: true,
                position_applied: true,
                size_applied: false,
                readback_xywh: Some((cmd.x, cmd.y, cmd.w, cmd.h)),
                readback_unreadable: false,
            };
        }
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
        Err(format!(
            "legacy snapshot refused for pid {} (native tests use scoped reads)",
            self.pid
        ))
    }

    fn bind_effect_hook(&mut self, hook: &tiler_macos::runtime_backend::EffectHook) {
        self.bound = hook.cell.load().map(|ctx| ctx.revision);
        self.inner
            .hooks
            .lock()
            .expect("mutex")
            .push(hook.frontmost.clone());
    }

    fn snapshot_scoped(&mut self) -> Result<ScopedBackendSnapshot, String> {
        let revision = self
            .bound
            .ok_or_else(|| format!("pid {}: no native context bound", self.pid))?;
        let mut snapshot = self
            .inner
            .scoped
            .lock()
            .expect("mutex")
            .get(&self.pid)
            .cloned()
            .ok_or_else(|| format!("no scoped snapshot for pid {}", self.pid))?;
        // A correct backend stamps the bound (current) revision it read
        // under; the engine re-fences it.
        snapshot.revision = revision;
        Ok(snapshot)
    }

    fn poll_scoped(&mut self, stop: &AtomicBool) -> Option<Result<ScopedBackendSnapshot, String>> {
        let rx = self.poll_rx.as_ref()?;
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(mut snapshot) => {
                if let Some(revision) = self.bound {
                    snapshot.revision = revision;
                }
                Some(Ok(snapshot))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.poll_rx = None;
                None
            }
        }
    }

    fn dispatch_focus(&mut self, cmd: &FocusCommand) -> AxCompletion {
        let _ = self.inner.focus_tx.send(cmd.clone());
        AxCompletion::Applied { elapsed_ms: 1 }
    }
}

struct FakeInput {
    rx: Mutex<Option<mpsc::Receiver<RawKeyEvent>>>,
}

impl InputSourceFactory for FakeInput {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        let rx = self
            .rx
            .lock()
            .expect("mutex")
            .take()
            .ok_or_else(|| "taken".to_owned())?;
        Ok(Box::new(FakeSource { rx }))
    }

    fn describe(&self) -> &'static str {
        "fake"
    }
}

struct FakeSource {
    rx: mpsc::Receiver<RawKeyEvent>,
}

impl InputSource for FakeSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
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

struct Fixture {
    handle: RuntimeHandle,
    inner: Arc<FakeInner>,
    applied_rx: mpsc::Receiver<AppliedRecord>,
    focus_rx: mpsc::Receiver<FocusCommand>,
    input_tx: mpsc::Sender<RawKeyEvent>,
}

fn scoped_config() -> RuntimeConfig {
    RuntimeConfig {
        topology_mode: TopologyMode::Native,
        ..RuntimeConfig::default()
    }
}

fn start(config: RuntimeConfig) -> Fixture {
    let (applied_tx, applied_rx) = mpsc::channel();
    let (focus_tx, focus_rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel();
    let inner = Arc::new(FakeInner {
        applied_tx,
        scoped: Mutex::new(BTreeMap::new()),
        poll_inboxes: Mutex::new(BTreeMap::new()),
        focus_tx,
        gates: Mutex::new(BTreeMap::new()),
        partial_once: Mutex::new(BTreeSet::new()),
        hooks: Mutex::new(Vec::new()),
    });
    let handle = Runtime::start(
        config,
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
        inner,
        applied_rx,
        focus_rx,
        input_tx,
    }
}

fn win(wid: u32) -> ObservedWindow {
    win_at(wid, 0, 0, 100, 100)
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
        hints: tiler_macos::runtime::WindowSizeHints::none(),
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

/// Scripted worker-published snapshot: windows plus exact Space membership
/// (the engine homes by this — never by geometry). Revision stamps at
/// publish time from the bound context.
fn scripted(
    windows: Vec<ObservedWindow>,
    spaces: Vec<Vec<u64>>,
    focused: Option<u32>,
) -> ScopedBackendSnapshot {
    assert_eq!(windows.len(), spaces.len());
    let meta = windows
        .iter()
        .zip(spaces.iter())
        .map(|(w, s)| meta(w.wid, s))
        .collect();
    ScopedBackendSnapshot {
        windows,
        focused_wid: focused,
        focus_cg_owned: focused.is_some(),
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta,
        excluded: 0,
        revision: 0,
    }
}

fn install(fixture: &Fixture, pid: u32, snapshot: ScopedBackendSnapshot) {
    fixture
        .inner
        .scoped
        .lock()
        .expect("mutex")
        .insert(pid, snapshot);
}

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

fn two_display_snapshot() -> topology::TopologySnapshot {
    topology::build_snapshot(
        &[
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ],
        &[
            row(Some("uuid-1"), &[11], Some(11)),
            row(Some("uuid-2"), &[21], Some(21)),
        ],
        Some(true),
    )
}

/// Work areas in integer units, mirroring the runtime derivation
/// (visible frames convert 1:1 here: primary at the origin).
fn work_areas() -> [(i32, i32, i32, i32); 2] {
    [(0, 0, 1440, 900), (1440, 0, 1440, 900)]
}

fn within(rect: (i32, i32, i32, i32), area: (i32, i32, i32, i32)) -> bool {
    let (x, y, w, h) = rect;
    let (ax, ay, aw, ah) = area;
    w > 0 && h > 0 && x >= ax && y >= ay && x + w <= ax + aw && y + h <= ay + ah
}

fn scoped_obs(
    pid: u32,
    generation: u64,
    seq: u64,
    members: Vec<(ObservedWindow, Vec<u64>)>,
    focused_wid: Option<u32>,
    revision: u64,
) -> ScopedObservation {
    let windows = members.iter().map(|(w, _)| *w).collect();
    let meta = members
        .iter()
        .map(|(w, spaces)| meta(w.wid, spaces))
        .collect();
    // Agreeing global evidence: frontmost + AX + CG all name the reporter.
    let (frontmost_pid, ax_focus) = match focused_wid {
        Some(wid) => (Some(pid), Some((pid, Some(wid)))),
        None => (None, None),
    };
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
    }
}

fn agreeing_evidence(pid: u32, wid: u32) -> FocusEvidence {
    FocusEvidence {
        frontmost_pid: Some(pid),
        frontmost_bundle: None,
        ax_focus: Some((pid, Some(wid))),
    }
}

fn wait_applied(fixture: &Fixture, window: &str) -> AppliedRecord {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("no applied frame for {window}");
        }
        match fixture.applied_rx.recv_timeout(remaining) {
            Ok(record) if record.2 == window => return record,
            Ok(_) => continue,
            Err(_) => panic!("no applied frame for {window}"),
        }
    }
}

fn wait_diagnostics(
    handle: &RuntimeHandle,
    wanted: impl Fn(&tiler_macos::runtime::RuntimeDiagnostics) -> bool,
    what: &str,
) -> tiler_macos::runtime::RuntimeDiagnostics {
    let deadline = std::time::Instant::now() + WAIT;
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

fn wait_focus(fixture: &Fixture) -> FocusCommand {
    fixture
        .focus_rx
        .recv_timeout(WAIT)
        .expect("no owner focus dispatch arrived")
}

fn shutdown_clean(fixture: &Fixture) {
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

// -- Tests -------------------------------------------------------------------

#[test]
fn native_plans_land_inside_actual_work_areas_divided_per_domain() {
    // Two displays, one window each, homed by exact Space membership (not
    // geometry: both observed rects start at the same integers).
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1)], vec![vec![11]], None),
    );
    install(
        &fixture,
        APP_B,
        scripted(vec![win(2)], vec![vec![21]], None),
    );
    fixture.handle.launch_app(APP_A).expect("launch A");
    fixture.handle.launch_app(APP_B).expect("launch B");
    assert!(fixture.handle.flush(FLUSH));

    // Both domains plan into their own actual work areas: the frames the
    // workers actuate prove per-domain core sessions (a skeleton session
    // would tile both windows into one 1920x1080 domain instead).
    let a = wait_applied(&fixture, "1001_1");
    let b = wait_applied(&fixture, "1002_2");
    let areas = work_areas();
    assert_eq!((a.0, a.4), (APP_A, revision));
    assert_eq!((b.0, b.4), (APP_B, revision));
    assert!(
        within(a.3, areas[0]),
        "display-1 window must stay in its work area: {:?} vs {:?}",
        a.3,
        areas[0]
    );
    assert!(
        within(b.3, areas[1]),
        "display-2 window must stay in its work area: {:?} vs {:?}",
        b.3,
        areas[1]
    );
    // The engine's own plan agrees (not just the actuation): exactly the
    // two homed windows, each inside its domain's work area — no skeleton.
    let desired = fixture.handle.last_desired_geometry();
    assert_eq!(desired.len(), 2);
    for frame in &desired {
        let rect = (frame.x, frame.y, frame.w, frame.h);
        let home = if frame.window == "1001_1" {
            areas[0]
        } else if frame.window == "1002_2" {
            areas[1]
        } else {
            panic!("unhomed window in native plan: {}", frame.window);
        };
        assert!(within(rect, home), "planned {:?} escapes {:?}", rect, home);
    }
    // Divided, never overlapping across domains.
    assert!(
        a.3.0 + a.3.2 <= b.3.0 || b.3.0 + b.3.2 <= a.3.0,
        "domains must not overlap: {:?} vs {:?}",
        a.3,
        b.3
    );
    // Both domain replies counted (two tiled plans, one per domain).
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics
            .core_replies
            .get("reconcile-tiled")
            .copied()
            .unwrap_or(0)
            >= 2,
        "each domain plans independently: {diagnostics:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn engine_omits_without_membership_never_infers_geometry() {
    // A window whose observed rect sits inside display 1's work area but
    // whose membership names a Space no domain homes (and a sticky window
    // spanning both currents) must omit: Space is never inferred from
    // geometry, and omission is counted evidence, never destruction.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            1,
            vec![
                (win_at(1, 100, 100, 200, 200), vec![99]),
                (win_at(2, 100, 100, 200, 200), vec![11, 21]),
                (win_at(3, 100, 100, 200, 200), vec![11]),
            ],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Only the exactly-homed member actuates.
    let applied = wait_applied(&fixture, "1001_3");
    assert_eq!(applied.2, "1001_3");
    assert!(
        fixture.applied_rx.try_recv().is_err(),
        "unhomed windows must never actuate"
    );
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.core_replies.get("reconcile-tiled").copied().unwrap_or(0) >= 1,
        "domain plan",
    );
    let _ = diagnostics;
    // The desired plan holds exactly the homed member.
    let desired = fixture.handle.last_desired_geometry();
    assert_eq!(
        desired
            .iter()
            .map(|f| f.window.as_str())
            .collect::<Vec<_>>(),
        vec!["1001_3"]
    );
    shutdown_clean(&fixture);
}

#[test]
fn partial_readback_converges_without_retry_storm() {
    // Worker-driven: the startup snapshot plans two windows; the first
    // apply goes partial; the post-apply readback (updated scripted truth)
    // converges instead of reasserting endlessly.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1), win(2)], vec![vec![11], vec![11]], None),
    );
    fixture
        .inner
        .partial_once
        .lock()
        .expect("mutex")
        .insert(APP_A);
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    // The partial completion is honest dispatch truth: counted under its own
    // token, never a pretend applied.
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.completions.get("partial-applied").copied().unwrap_or(0) >= 1,
        "partial completion",
    );
    assert_eq!(diagnostics.completions.get("partial-applied"), Some(&1));
    // Quiesce: no storm follows (the count stays exactly 1 while the
    // readback converges the plan instead of refighting it).
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fixture
            .handle
            .diagnostics()
            .completions
            .get("partial-applied"),
        Some(&1),
        "partial must not repeat: {:?}",
        fixture.handle.diagnostics()
    );
    shutdown_clean(&fixture);
}

#[test]
fn live_topology_path_degrades_closed() {
    // The live `update_topology` entry resolves Space types through the
    // real read-only SLS path: with fixture Space IDs it degrades to
    // skipped displays (never a guessed domain, never a panic) on every
    // platform, while the revision fence still arms.
    let fixture = start(scoped_config());
    let snapshot = two_display_snapshot();
    let revision = fixture.handle.update_topology(snapshot);
    assert_eq!(fixture.handle.topology_revision(), Some(revision));
    shutdown_clean(&fixture);
}

#[test]
fn observer_reread_publishes_fresh_truth_without_storm() {
    // Worker-driven streaming: the startup snapshot plans, then an
    // observer notification delivers a reread with moved geometry. The
    // engine consumes it (publication count grows) and settles — no
    // republish storm follows.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let (poll_tx, poll_rx) = mpsc::channel();
    fixture
        .inner
        .poll_inboxes
        .lock()
        .expect("mutex")
        .insert(APP_A, poll_rx);
    install(
        &fixture,
        APP_A,
        scripted(vec![win_at(1, 0, 0, 100, 100)], vec![vec![11]], None),
    );
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    let _ = wait_applied(&fixture, "1001_1");

    // Live change: the window moves. The scripted truth moves first (later
    // reads agree), then the observer notification delivers the reread —
    // so the post-apply readback confirms instead of reverting.
    let moved = scripted(vec![win_at(1, 300, 300, 100, 100)], vec![vec![11]], None);
    install(&fixture, APP_A, moved.clone());
    poll_tx.send(moved).expect("observer notification delivers");
    wait_diagnostics(
        &fixture.handle,
        |d| d.observations_published >= 2,
        "reread publication",
    );
    assert!(fixture.handle.flush(FLUSH));
    // Settles: the publication count holds (fresh truth consumed once),
    // and the plan still covers exactly the homed member.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fixture.handle.diagnostics().observations_published,
        2,
        "reread must not storm: {:?}",
        fixture.handle.diagnostics()
    );
    assert_eq!(
        fixture
            .handle
            .last_desired_geometry()
            .iter()
            .map(|f| f.window.as_str())
            .collect::<Vec<_>>(),
        vec!["1001_1"]
    );
    shutdown_clean(&fixture);
}

#[test]
fn hung_app_isolates_while_other_app_progresses() {
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        cond: std::sync::Condvar::new(),
    });
    fixture
        .inner
        .gates
        .lock()
        .expect("mutex")
        .insert(APP_A, Arc::clone(&gate));
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1)], vec![vec![11]], None),
    );
    install(
        &fixture,
        APP_B,
        scripted(vec![win(2)], vec![vec![21]], None),
    );
    fixture.handle.launch_app(APP_A).expect("launch A");
    fixture.handle.launch_app(APP_B).expect("launch B");
    assert!(fixture.handle.flush(FLUSH));
    // B progresses while A is hung inside AX.
    let b = wait_applied(&fixture, "1002_2");
    assert_eq!(b.0, APP_B);
    *gate.released.lock().expect("mutex") = true;
    gate.cond.notify_all();
    let a = wait_applied(&fixture, "1001_1");
    assert_eq!(a.0, APP_A);
    shutdown_clean(&fixture);
}

#[test]
fn display_change_mid_plan_supersedes_frames_and_queued_focus() {
    let fixture = start(scoped_config());
    let first = two_display_snapshot();
    let revision = fixture.handle.update_topology_with(first, &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    // Three windows so a focus step resolves; focus queues one outbox
    // record and one owner-slot command.
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            1,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(3), vec![11]),],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let flags = cg_flag_bits(ProbeModifier::Ctrl).0;
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x04,
        })
        .expect("send focus chord");
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_forwarded >= 1,
        "focus chord forwarded",
    );
    assert!(fixture.handle.flush(FLUSH));
    // Display change before any drain: the queued outbox record is stale,
    // and the owner-slot command supersedes without dispatching.
    let mut moved = two_display_snapshot();
    moved.displays[0].scale = 3.0;
    let revision2 =
        topology::fingerprint_snapshot(&moved.displays, &moved.rows, moved.separate_spaces);
    moved.revision = revision2;
    fixture.handle.update_topology_with(moved, &user_space);
    let drained = fixture.handle.drain_focus_requests();
    assert!(
        drained.is_empty(),
        "superseded focus must drain stale, got {drained:?}"
    );
    // Stale-revision observations fence too: the old revision no longer
    // admits.
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            2,
            vec![(win(1), vec![11])],
            Some(1),
            revision
        )),
        SubmitOutcome::StaleGeneration
    );
    shutdown_clean(&fixture);
}

#[test]
fn space_change_retires_dead_domain_and_reseeds() {
    // Display 1 moves from Space 11 to Space 12: the space-11 session must
    // retire (ring evidence) and space-12 must seed fresh and tile inside
    // display 1's actual work area. Display 2 (Space 21) is undisturbed.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1)], vec![vec![11]], None),
    );
    install(
        &fixture,
        APP_B,
        scripted(vec![win(2)], vec![vec![21]], None),
    );
    fixture.handle.launch_app(APP_A).expect("launch A");
    fixture.handle.launch_app(APP_B).expect("launch B");
    assert!(fixture.handle.flush(FLUSH));
    let _ = wait_applied(&fixture, "1001_1");
    let _ = wait_applied(&fixture, "1002_2");
    while fixture.applied_rx.try_recv().is_ok() {}

    let mut moved = two_display_snapshot();
    moved.rows[0].spaces = vec![11, 12];
    moved.rows[0].current = Some(12);
    moved.revision =
        topology::fingerprint_snapshot(&moved.displays, &moved.rows, moved.separate_spaces);
    let revision2 = fixture.handle.update_topology_with(moved, &user_space);
    // Workers re-read under the new revision (same observed windows, new
    // membership evidence arrives with the next snapshots).
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1)], vec![vec![12]], None),
    );
    let gen_a = fixture.handle.app_generation(APP_A).expect("gen A");
    let gen_b = fixture.handle.app_generation(APP_B).expect("gen B");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            2,
            vec![(win(1), vec![12])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            2,
            vec![(win(2), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Both windows re-actuate inside their (unchanged) work areas.
    let a = wait_applied(&fixture, "1001_1");
    let b = wait_applied(&fixture, "1002_2");
    let areas = work_areas();
    assert!(within(a.3, areas[0]), "reseeded domain tiles d1: {:?}", a.3);
    assert!(within(b.3, areas[1]), "stable domain tiles d2: {:?}", b.3);
    // Retirement evidence is on the ring (the space-11 session is gone).
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "domain retirement must be recorded"
    );
    shutdown_clean(&fixture);
}

#[test]
fn out_of_scope_config_and_unknown_focus_never_actuate() {
    // Ghostty in the allowlist fails validation before any thread exists.
    let bad = RuntimeConfig {
        allowed_bundles: vec!["com.mitchellh.ghostty".to_owned()],
        ..RuntimeConfig::default()
    };
    assert!(bad.validate().is_err());
    let own = RuntimeConfig {
        allowed_bundles: vec![tiler_macos::BUNDLE_ID.to_owned()],
        ..RuntimeConfig::default()
    };
    assert!(own.validate().is_err());
    // A named everyday scope (TextEdit/Calculator class) validates clean.
    let named = RuntimeConfig {
        allowed_bundles: vec![
            "com.apple.TextEdit".to_owned(),
            "com.apple.Calculator".to_owned(),
        ],
        ..RuntimeConfig::default()
    };
    assert!(named.validate().is_ok());

    // Background app-local focus never becomes global truth: the scoped
    // observation arrives unfocused, so input has no subject and the core
    // is never contacted for it.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let disagreeing = ScopedObservation {
        pid: APP_A,
        generation: gen_a,
        seq: 1,
        windows: vec![win(1)],
        focused_wid: Some(1),
        revision,
        // App-local claims focus, but frontmost + AX name another app.
        frontmost_pid: Some(9999),
        ax_focus: Some((9999, Some(1))),
        cg_owned: true,
        meta: vec![meta(1, &[11])],
    };
    assert_eq!(
        fixture.handle.submit_scoped_observation(disagreeing),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics.stale_dropped >= 1,
        "focus disagreement must be counted: {diagnostics:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn focus_dispatches_through_owner_slot_with_exact_target() {
    // Worker-published snapshot plus agreeing evidence promotes focus;
    // the input focus step then dispatches through the owning worker's
    // slot with the exact window target (never a foreign window), while
    // the outbox keeps its drained record for the controller.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(
            vec![
                win_at(1, 0, 0, 480, 900),
                win_at(2, 480, 0, 480, 900),
                win_at(3, 960, 0, 480, 900),
            ],
            vec![vec![11], vec![11], vec![11]],
            Some(1),
        ),
    );
    // Evidence BEFORE launch: the worker stamps it onto its startup
    // snapshot, promoting focus on first publish (a later evidence update
    // would arrive after the snapshot and withhold until the next one).
    fixture
        .handle
        .update_focus_evidence(agreeing_evidence(APP_A, 1));
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    // Sanity: the promoted focus lands in the desired plan first.
    wait_applied(&fixture, "1001_1");

    let flags = cg_flag_bits(ProbeModifier::Ctrl).0;
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x04,
        })
        .expect("send focus chord");
    // The SAME owner dispatches the exact target window.
    let cmd = wait_focus(&fixture);
    assert_eq!((cmd.pid, cmd.wid), (APP_A, 2));
    assert_eq!(cmd.route, FocusRoute::Slps);
    assert_eq!(cmd.window, "1001_2");
    // Dispatch truth lands on the focus-result lane (never as arrival).
    wait_diagnostics(
        &fixture.handle,
        |d| {
            fixture
                .handle
                .log_snapshot()
                .iter()
                .any(|e| e.kind == "focus-result" && e.outcome == "applied")
                && d.completions.get("applied").copied().unwrap_or(0) >= 1
        },
        "focus dispatch truth",
    );
    // The outbox keeps its controller-visible record too.
    let drained = fixture.handle.drain_focus_requests();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].window, "1001_2");
    assert!(!drained[0].verified);
    shutdown_clean(&fixture);
}

#[test]
fn worker_focus_withholds_without_evidence() {
    // A worker-published focused window with no main-thread evidence never
    // promotes: input has no subject (refused-no-focus), so the core is
    // never contacted for it.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1)], vec![vec![11]], Some(1)),
    );
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(
        fixture.handle.request_float_toggle(),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = wait_diagnostics(
        &fixture.handle,
        |d| d.input_refused_no_focus >= 1,
        "focus withheld without evidence",
    );
    assert!(
        !diagnostics
            .core_replies
            .keys()
            .any(|k| k.starts_with("toggle-float")),
        "withheld focus must never reach the core: {diagnostics:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn float_toggle_uses_existing_core_command() {
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            1,
            vec![(win(1), vec![11])],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(
        fixture.handle.request_float_toggle(),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    let toggles: u64 = diagnostics
        .core_replies
        .iter()
        .filter(|(k, _)| k.starts_with("toggle-float"))
        .map(|(_, v)| v)
        .sum();
    assert_eq!(toggles, 1, "toggle must reach the core: {diagnostics:?}");

    // Exact float chord forwards through the input owner too.
    let flags = cg_flag_bits(ProbeModifier::Ctrl).0;
    let baseline = fixture.handle.diagnostics().input_forwarded;
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x06,
        })
        .expect("send float chord");
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_forwarded > baseline,
        "float chord forwarded",
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    let toggles: u64 = diagnostics
        .core_replies
        .iter()
        .filter(|(k, _)| k.starts_with("toggle-float"))
        .map(|(_, v)| v)
        .sum();
    assert_eq!(
        toggles, 2,
        "chord toggle must reach the core: {diagnostics:?}"
    );
    assert_eq!(FocusRoute::default(), FocusRoute::Slps);
    shutdown_clean(&fixture);
}

fn wait_hooks_contain(fixture: &Fixture, wanted: &HookFrontmost, what: &str) {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        {
            let hooks = fixture.inner.hooks.lock().expect("mutex");
            if hooks.iter().any(|seen| seen == wanted) {
                return;
            }
        }
        if std::time::Instant::now() >= deadline {
            panic!("effect hook never carried {what}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn effect_hook_rebinds_frontmost_every_iteration() {
    // The worker rebinds its effect hook every multiplex iteration (not
    // just on topology drift): a frontmost update must reach the bound
    // hook with no display/Space change at all, so fences never act on a
    // stale-cached frontmost (and a cleared cell propagates too).
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(vec![win(1)], vec![vec![11]], Some(1)),
    );
    fixture
        .handle
        .update_focus_evidence(agreeing_evidence(APP_A, 1));
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    wait_hooks_contain(&fixture, &Some((APP_A, None)), "initial frontmost");
    // New frontmost, nothing else changes: the hook must carry it without
    // any topology drift.
    fixture.handle.update_focus_evidence(FocusEvidence {
        frontmost_pid: Some(7777),
        frontmost_bundle: None,
        ax_focus: None,
    });
    wait_hooks_contain(&fixture, &Some((7777, None)), "refreshed frontmost");
    shutdown_clean(&fixture);
}

#[test]
fn frontmost_change_clears_and_repromotes_without_backend_change() {
    // A workspace frontmost change alone (no AX notification, no frames,
    // backend snapshot static) still re-resolves the global-focus verdict:
    // disagreement clears observed focus (input loses its subject), and a
    // later agreement re-promotes it. Steady frontmost never reasserts.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    install(
        &fixture,
        APP_A,
        scripted(
            vec![
                win_at(1, 0, 0, 480, 900),
                win_at(2, 480, 0, 480, 900),
                win_at(3, 960, 0, 480, 900),
            ],
            vec![vec![11], vec![11], vec![11]],
            Some(1),
        ),
    );
    fixture
        .handle
        .update_focus_evidence(agreeing_evidence(APP_A, 1));
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    wait_applied(&fixture, "1001_1");
    let baseline_published = fixture.handle.diagnostics().observations_published;

    // Frontmost moves elsewhere while the backend snapshot (and its AX
    // leg) stands still: the worker must republish and the engine must
    // clear, so the next focus chord has no subject.
    fixture.handle.update_focus_evidence(FocusEvidence {
        frontmost_pid: Some(9999),
        frontmost_bundle: None,
        ax_focus: Some((APP_A, Some(1))),
    });
    wait_diagnostics(
        &fixture.handle,
        |d| d.observations_published > baseline_published,
        "frontmost-change republish",
    );
    assert!(fixture.handle.flush(FLUSH));
    let flags = cg_flag_bits(ProbeModifier::Ctrl).0;
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x04,
        })
        .expect("send focus chord");
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_refused_no_focus >= 1,
        "cleared focus refuses input",
    );

    // Agreement returns: republish once more and the subject re-promotes
    // (the SAME owner dispatches the exact target window again).
    let republished = fixture.handle.diagnostics().observations_published;
    fixture
        .handle
        .update_focus_evidence(agreeing_evidence(APP_A, 1));
    wait_diagnostics(
        &fixture.handle,
        |d| d.observations_published > republished,
        "agreement republish",
    );
    assert!(fixture.handle.flush(FLUSH));
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x04,
        })
        .expect("send focus chord");
    let cmd = wait_focus(&fixture);
    assert_eq!((cmd.pid, cmd.wid), (APP_A, 2));

    // Steady frontmost never reasserts: no further republish while nothing
    // changes.
    assert!(fixture.handle.flush(FLUSH));
    let settled = fixture.handle.diagnostics().observations_published;
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        fixture.handle.diagnostics().observations_published,
        settled,
        "no busy reassertion on steady frontmost"
    );
    shutdown_clean(&fixture);
}
