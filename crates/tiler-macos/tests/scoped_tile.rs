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
    AxCompletion, EngineManagedFacts, FocusCommand, FocusEvidence, FocusRoute, ObservedWindow,
    Runtime, RuntimeConfig, RuntimeHandle, ScopedBackendSnapshot, ScopedObservation,
    ScopedWindowMeta, ShutdownOutcome, SubmitOutcome, TopologyMode,
};
use tiler_macos::runtime_backend::{
    AxBackend, AxBackendFactory, InputSource, InputSourceFactory, RawKeyEvent,
};
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
    // Fake attests a fresh complete CG read for its own windows.
    let mut cg_owner_wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
    cg_owner_wids.sort_unstable();
    cg_owner_wids.dedup();
    ScopedBackendSnapshot {
        windows,
        focused_wid: focused,
        focus_cg_owned: focused.is_some(),
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta,
        excluded: 0,
        excluded_reasons: Vec::new(),
        cg_complete: true,
        cg_owner_wids,
        cg_alive: Vec::new(),
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
    let windows: Vec<ObservedWindow> = members.iter().map(|(w, _)| *w).collect();
    // Agreeing global evidence: frontmost + AX + CG all name the reporter.
    let (frontmost_pid, ax_focus) = match focused_wid {
        Some(wid) => (Some(pid), Some((pid, Some(wid)))),
        None => (None, None),
    };
    // Fake attests a fresh complete CG read for its own windows.
    let mut cg_owner_wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
    cg_owner_wids.sort_unstable();
    cg_owner_wids.dedup();
    let meta = members
        .iter()
        .map(|(w, spaces)| meta(w.wid, spaces))
        .collect();
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

/// Wait for applied frames for two windows regardless of arrival order
/// (deadline-bounded). Sequential `wait_applied` calls race when two
/// workers apply concurrently: the first wait discards the second
/// window's only record and the second wait starves. Returns both
/// records in the requested order.
fn wait_applied_pair(
    fixture: &Fixture,
    first: &str,
    second: &str,
) -> (AppliedRecord, AppliedRecord) {
    let deadline = std::time::Instant::now() + WAIT;
    let mut seen: [Option<AppliedRecord>; 2] = [None, None];
    while seen[0].is_none() || seen[1].is_none() {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("no applied frames for {first} and {second}");
        }
        match fixture.applied_rx.recv_timeout(remaining) {
            Ok(record) if record.2 == first && seen[0].is_none() => seen[0] = Some(record),
            Ok(record) if record.2 == second && seen[1].is_none() => seen[1] = Some(record),
            Ok(_) => continue,
            Err(_) => panic!("no applied frames for {first} and {second}"),
        }
    }
    (
        seen[0].clone().expect("seen"),
        seen[1].clone().expect("seen"),
    )
}

/// Wait for the next applied frame for `window` whose rect equals
/// `expected`: superseded records from pre-drain dispatches (same window,
/// older geometry landing after the test's channel drain — flush fences
/// engine jobs, not worker applies, so a slow worker's apply can arrive
/// late under load) are skipped instead of poisoning exact-frame oracles.
/// Bounded by WAIT like [`wait_applied`]; a genuine never-convergence
/// times out loudly with the skip count instead of asserting on stale
/// geometry.
fn wait_applied_to(
    fixture: &Fixture,
    window: &str,
    expected: (i32, i32, i32, i32),
    what: &str,
) -> AppliedRecord {
    let deadline = std::time::Instant::now() + WAIT;
    let mut skipped = 0u32;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!(
                "no applied frame for {window} converging to {expected:?} ({what}; skipped {skipped} stale)"
            );
        }
        match fixture.applied_rx.recv_timeout(remaining) {
            Ok(record) if record.2 == window && record.3 == expected => return record,
            Ok(record) if record.2 == window => skipped += 1,
            Ok(_) => continue,
            Err(_) => panic!(
                "no applied frame for {window} converging to {expected:?} ({what}; skipped {skipped} stale)"
            ),
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

#[test]
fn send_effect_owner_topology_never_touches_main_thread_displays() {
    // Display legs arrive as startup/notification publications; the owner
    // joins them with fresh SLS rows on its own thread.
    use tiler_macos::workspace_send_native::{OsSendReaders, SendNativeReaders};
    let fixture = start(scoped_config());
    let probe = fixture.handle.clone();
    let err = std::thread::spawn(move || {
        let mut readers = OsSendReaders::live(probe);
        readers.fresh_topology().unwrap_err()
    })
    .join()
    .expect("owner joins");
    assert!(
        err.contains("unpublished"),
        "unpublished display inputs refuse by name: {err}"
    );
    assert!(
        !err.contains("requires the main thread"),
        "effect owner never invokes the main-thread-only display read: {err}"
    );
    // Steady-state topology publishes never move the notification slot.
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    assert!(
        fixture.handle.published_display_inputs().is_none(),
        "poll-path topology publish leaves display inputs unpublished"
    );
    // Startup/notification publication is latest-wins.
    fixture.handle.publish_display_inputs(&two_space_snapshot());
    let first = fixture
        .handle
        .published_display_inputs()
        .expect("display inputs published");
    assert_eq!(first.displays.len(), 1);
    assert_eq!(first.separate, Some(true));
    fixture
        .handle
        .publish_display_inputs(&two_display_snapshot());
    let second = fixture
        .handle
        .published_display_inputs()
        .expect("display inputs republished");
    assert_eq!(second.displays.len(), 2);
    // A later poll-path publish with different legs leaves the slot alone.
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    assert_eq!(
        fixture
            .handle
            .published_display_inputs()
            .expect("slot intact")
            .displays
            .len(),
        2,
        "poll-path publish never overwrites notification slot"
    );
    // Published inputs: the display leg passes on the spawned thread. Any
    // refusal names only the fresh SLS legs, never the display snapshot.
    let probe = fixture.handle.clone();
    let outcome = std::thread::spawn(move || {
        let mut readers = OsSendReaders::live(probe);
        readers.fresh_topology().map(|topo| topo.revision)
    })
    .join()
    .expect("owner joins");
    match outcome {
        Ok(_) => {}
        Err(err) => {
            assert!(
                !err.contains("display snapshot"),
                "display leg passes off-main; only fresh SLS legs may refuse: {err}"
            );
            assert!(
                err.contains("space rows"),
                "an off-main refusal names the fresh SLS legs: {err}"
            );
        }
    }
    // Invalidation clears back to refusal: stale inputs never authorize.
    fixture.handle.invalidate_topology();
    assert!(
        fixture.handle.published_display_inputs().is_none(),
        "invalidation clears published display inputs"
    );
    shutdown_clean(&fixture);
}

#[test]
fn send_display_slot_recovers_after_invalidation_without_notification() {
    // Transient unreadable snapshot clears the slot; the next good snapshot
    // restores it with no screen notification, or numbered sends would
    // refuse forever in a static session. The test drives the producer's
    // own condition (absent slot republishes, populated quiet poll does
    // not) rather than the setter alone.
    let fixture = start(scoped_config());
    fixture
        .handle
        .publish_display_inputs(&two_display_snapshot());
    assert!(
        fixture.handle.has_display_inputs(),
        "startup publish fills the slot"
    );
    fixture.handle.invalidate_topology();
    assert!(
        !fixture.handle.has_display_inputs(),
        "unreadable snapshot clears the slot"
    );
    // Next good snapshot, no notification: the absent slot republishes.
    if !fixture.handle.has_display_inputs() {
        fixture
            .handle
            .publish_display_inputs(&two_display_snapshot());
    }
    assert_eq!(
        fixture
            .handle
            .published_display_inputs()
            .expect("slot recovered")
            .displays
            .len(),
        2,
        "cleared slot recovers from the next good snapshot"
    );
    // Quiet poll with a populated slot leaves it untouched.
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    assert!(
        fixture.handle.has_display_inputs(),
        "quiet poll keeps the recovered slot"
    );
    assert_eq!(
        fixture
            .handle
            .published_display_inputs()
            .expect("slot intact")
            .displays
            .len(),
        2,
        "quiet poll never moves a populated slot"
    );
    shutdown_clean(&fixture);
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
    let (a, b) = wait_applied_pair(&fixture, "1001_1", "1002_2");
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
    // Both observations dispatch (one complete world reply each — no
    // domain fragments).
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics
            .core_replies
            .get("reconcile-accepted")
            .copied()
            .unwrap_or(0)
            >= 2,
        "each observation dispatches independently: {diagnostics:?}"
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
        |d| {
            d.core_replies
                .get("reconcile-accepted")
                .copied()
                .unwrap_or(0)
                >= 1
        },
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
fn managed_facts_query_serves_current_engine_truth() {
    // The dedicated effect owner's authority over the control lane (strict
    // FIFO with observations): current generation, homing source plus
    // actuation, retained fullscreen, minimized exclusion, and the retained
    // revision. Unknown windows, unhomed windows, and terminated lifetimes
    // publish nothing — native OS reads supplement these facts on the
    // owner, never replace them.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    // Unknown before any observation: never guessed.
    assert!(
        fixture.handle.query_managed_facts("1001_1", WAIT).is_none(),
        "no facts before observation"
    );
    let mut hidden_full = win_at(2, 100, 100, 200, 200);
    hidden_full.fullscreen = true;
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            1,
            vec![
                (win_at(1, 100, 100, 200, 200), vec![11]),
                (hidden_full, vec![12]),
                (win_at(3, 100, 100, 200, 200), vec![99]),
            ],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Current-domain member: full facts, actuated.
    let facts: EngineManagedFacts = fixture
        .handle
        .query_managed_facts("1001_1", WAIT)
        .expect("homed window publishes");
    assert_eq!(facts.generation, gen_a);
    assert_eq!(facts.source.workspace.0, "space-11");
    assert!(facts.source_current, "Space 11 is current");
    assert!(!facts.fullscreen);
    assert!(!facts.minimized);
    assert_eq!(facts.revision, revision);
    // Hidden-domain member publishes but withholds actuation; retained
    // fullscreen rides truthfully.
    let hidden = fixture
        .handle
        .query_managed_facts("1001_2", WAIT)
        .expect("hidden member publishes");
    assert!(!hidden.source_current, "Space 12 is hidden");
    assert!(hidden.fullscreen);
    // Unhomed window: nothing.
    assert!(
        fixture.handle.query_managed_facts("1001_3", WAIT).is_none(),
        "unhomed window publishes nothing"
    );
    // Terminate retires the lifetime: nothing publishes afterward.
    let _ = fixture.handle.terminate_app(APP_A);
    assert!(fixture.handle.flush(FLUSH));
    assert!(
        fixture.handle.query_managed_facts("1001_1", WAIT).is_none(),
        "terminated lifetime publishes nothing"
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
fn space_switch_preserves_hidden_layout_without_retirement() {
    // Display 1 knows Spaces 11+12 (11 current); display 2 stays on 21.
    // Two windows tile on 11 with a moved (nontrivial) layout; switching
    // current to 12 tiles the newly-visible window there while 11 keeps
    // its exact session (no retirement, no frames or focus into the
    // hidden Space); switching back restores the exact moved frames.
    let fixture = start(scoped_config());
    let mut first = two_display_snapshot();
    first.rows[0].spaces = vec![11, 12];
    first.rows[0].current = Some(11);
    first.revision =
        topology::fingerprint_snapshot(&first.displays, &first.rows, first.separate_spaces);
    let revision = fixture.handle.update_topology_with(first, &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    // A tiles three windows on 11 (a real tree, not a single split);
    // B holds display 2's witness on 21.
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(4), vec![11]),],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(9), vec![21])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let _ = wait_applied(&fixture, "1001_1");
    let _ = wait_applied(&fixture, "1001_2");
    let _ = wait_applied(&fixture, "1001_4");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Nontrivial layout: focus the leftmost window and move it right.
    // Leftmost-picked (never assumed order) so the move always has a
    // right neighbor; with three windows grouping/swapping always
    // reflows frames (a two-window group would be a geometry no-op).
    let leftmost = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .filter(|f| f.window.starts_with("1001_"))
        .min_by_key(|f| f.x)
        .map(|f| f.window.clone())
        .expect("three tiled windows");
    let focus_wid: u32 = leftmost
        .split('_')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("numeric wid");
    let before: Vec<(String, (i32, i32, i32, i32))> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .filter(|f| f.window.starts_with("1001_"))
        .map(|f| (f.window.clone(), (f.x, f.y, f.w, f.h)))
        .collect();
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            11,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(4), vec![11]),],
            Some(focus_wid),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    while fixture.applied_rx.try_recv().is_ok() {}
    // Fence the input thread's handoff first: its engine job can queue
    // behind the flush barrier otherwise, and the flush below would prove
    // nothing about the move.
    let baseline = fixture.handle.diagnostics().input_forwarded;
    let flags = cg_flag_bits(ProbeModifier::Ctrl).0;
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x05,
        })
        .expect("send move chord");
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_forwarded > baseline,
        "move chord forwarded",
    );
    assert!(fixture.handle.flush(FLUSH));
    // Expected frames read from engine state (deterministic post-barrier
    // truth); the channel waits below converge to exactly these, skipping
    // superseded pre-drain records from slow workers instead of poisoning
    // the oracle with stale geometry.
    let planned_by_window = |window: &str| {
        fixture
            .handle
            .last_desired_geometry()
            .iter()
            .find(|f| f.window == window)
            .map(|f| (f.x, f.y, f.w, f.h))
            .expect("planned window")
    };
    let mut moved_plan: Vec<(String, (i32, i32, i32, i32))> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| (f.window.clone(), (f.x, f.y, f.w, f.h)))
        .collect();
    moved_plan.sort();
    let m1 = wait_applied_to(
        &fixture,
        "1001_1",
        planned_by_window("1001_1"),
        "moved layout",
    );
    let m2 = wait_applied_to(
        &fixture,
        "1001_2",
        planned_by_window("1001_2"),
        "moved layout",
    );
    let m4 = wait_applied_to(
        &fixture,
        "1001_4",
        planned_by_window("1001_4"),
        "moved layout",
    );
    let mut moved_rects: Vec<(String, (i32, i32, i32, i32))> =
        [m1, m2, m4].iter().map(|r| (r.2.clone(), r.3)).collect();
    moved_rects.sort();
    let mut before_sorted = before.clone();
    before_sorted.sort();
    assert_ne!(moved_rects, before_sorted, "move must rearrange the layout");
    // Exact moved frames per window for the return-trip oracle below.
    let moved_by_window = |window: &str| {
        moved_rects
            .iter()
            .find(|(w, _)| w == window)
            .map(|(_, r)| *r)
            .expect("moved window")
    };
    while fixture.applied_rx.try_recv().is_ok() {}
    // Switch display 1 to 12 (11 stays known): A reports its triple
    // hidden plus a new window on 12; B is undisturbed.
    let mut moved = two_display_snapshot();
    moved.rows[0].spaces = vec![11, 12];
    moved.rows[0].current = Some(12);
    moved.revision =
        topology::fingerprint_snapshot(&moved.displays, &moved.rows, moved.separate_spaces);
    let revision2 = fixture.handle.update_topology_with(moved, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            20,
            vec![
                (win(1), vec![11]),
                (win(2), vec![11]),
                (win(4), vec![11]),
                (win(3), vec![12]),
            ],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            20,
            vec![(win(9), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Only the current Space actuates: the newly-visible window tiles in
    // display 1's work area; the hidden triple takes no writes.
    let c = wait_applied(&fixture, "1001_3");
    let areas = work_areas();
    assert!(
        within(c.3, areas[0]),
        "switched-to Space tiles d1: {:?}",
        c.3
    );
    std::thread::sleep(Duration::from_millis(300));
    let mut hidden_writes = Vec::new();
    while let Ok(record) = fixture.applied_rx.try_recv() {
        // Generation-gated: only frames planned under the switched
        // revision count — flush fences engine jobs, not worker applies,
        // so superseded pre-switch records can land late under load.
        if record.2.starts_with("1001_") && record.2 != "1001_3" && record.4 == revision2 {
            hidden_writes.push(record);
        }
    }
    assert!(
        hidden_writes.is_empty(),
        "hidden windows take no writes: {hidden_writes:?}"
    );
    assert!(
        fixture.focus_rx.try_recv().is_err(),
        "no focus dispatches around a switch"
    );
    // A mere switch retires nothing (the space-11 session survives).
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "switch must not retire the hidden domain"
    );
    // The desired plan covers exactly the current Spaces (hidden
    // reconciles silently, never desired).
    let mut desired: Vec<String> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| f.window.clone())
        .collect();
    desired.sort();
    assert_eq!(desired, vec!["1001_3".to_owned(), "1002_9".to_owned()]);
    // Switch back to 11: the triple returns with its exact moved frames.
    let mut back = two_display_snapshot();
    back.rows[0].spaces = vec![11, 12];
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision3 = fixture.handle.update_topology_with(back, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            30,
            vec![
                (win(1), vec![11]),
                (win(2), vec![11]),
                (win(4), vec![11]),
                (win(3), vec![12]),
            ],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            30,
            vec![(win(9), vec![21])],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let r1 = wait_applied_to(
        &fixture,
        "1001_1",
        moved_by_window("1001_1"),
        "return converges to moved layout",
    );
    let r2 = wait_applied_to(
        &fixture,
        "1001_2",
        moved_by_window("1001_2"),
        "return converges to moved layout",
    );
    let r4 = wait_applied_to(
        &fixture,
        "1001_4",
        moved_by_window("1001_4"),
        "return converges to moved layout",
    );
    assert_eq!(
        r1.3,
        moved_by_window("1001_1"),
        "return restores exact moved layout"
    );
    assert_eq!(
        r2.3,
        moved_by_window("1001_2"),
        "return restores exact moved layout"
    );
    assert_eq!(
        r4.3,
        moved_by_window("1001_4"),
        "return restores exact moved layout"
    );
    // Engine-state round-trip proof (channel-free): the return plan over
    // the parked session is exactly the moved plan — same tree survived
    // the switch, not a reseed. Compared on display 1's windows: the move
    // plan snapshot covers only its own domain (last-writer-wins desired),
    // while the return plan also carries display 2's undisturbed witness.
    let display1_frames = |frames: Vec<(String, (i32, i32, i32, i32))>| {
        let mut v: Vec<_> = frames
            .into_iter()
            .filter(|(w, _)| w.starts_with("1001_"))
            .collect();
        v.sort();
        v
    };
    let returned_plan: Vec<(String, (i32, i32, i32, i32))> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| (f.window.clone(), (f.x, f.y, f.w, f.h)))
        .collect();
    assert_eq!(
        display1_frames(returned_plan),
        display1_frames(moved_plan),
        "return plan equals moved plan: session survived the switch"
    );
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "round trip retires nothing"
    );
    shutdown_clean(&fixture);
}

#[test]
fn deleted_space_retires_domain_and_reseeds() {
    // Display 1's Space 11 is DELETED (row drops it, current becomes 12):
    // the space-11 session must retire (ring evidence) and space-12 must
    // seed fresh and tile inside display 1's actual work area. Display 2
    // (Space 21) is undisturbed. Companion to the switch test above, which
    // proves a mere current change retires nothing.
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
    let _ = wait_applied_pair(&fixture, "1001_1", "1002_2");
    while fixture.applied_rx.try_recv().is_ok() {}

    let mut moved = two_display_snapshot();
    moved.rows[0].spaces = vec![12];
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
            20,
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
            20,
            vec![(win(2), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Both windows re-actuate inside their (unchanged) work areas.
    let (a, b) = wait_applied_pair(&fixture, "1001_1", "1002_2");
    let areas = work_areas();
    assert!(within(a.3, areas[0]), "reseeded domain tiles d1: {:?}", a.3);
    assert!(within(b.3, areas[1]), "stable domain tiles d2: {:?}", b.3);
    // Convergence evidence is on the ring (the engine adopted the new
    // topology through a full world report; the space-11 session retired
    // core-internally — its Space left the display, not merely the
    // foreground — while the re-actuation above proves the reseed).
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "topology-adopted"),
        "deleted Space must converge its removal"
    );
    shutdown_clean(&fixture);
}

#[test]
fn offcurrent_close_prunes_and_return_converges() {
    // A window closed while its Space is hidden prunes from the hidden
    // session without disturbing the current Space; returning converges
    // the survivor (which now fills the domain alone).
    let fixture = start(scoped_config());
    let mut first = two_display_snapshot();
    first.rows[0].spaces = vec![11, 12];
    first.rows[0].current = Some(11);
    first.revision =
        topology::fingerprint_snapshot(&first.displays, &first.rows, first.separate_spaces);
    let revision = fixture.handle.update_topology_with(first, &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11]), (win(2), vec![11]),],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(9), vec![21])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let f1 = wait_applied(&fixture, "1001_1");
    let _ = wait_applied(&fixture, "1001_2");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Switch to the empty Space 12: the hidden pair takes no writes and
    // nothing retires.
    let mut moved = two_display_snapshot();
    moved.rows[0].spaces = vec![11, 12];
    moved.rows[0].current = Some(12);
    moved.revision =
        topology::fingerprint_snapshot(&moved.displays, &moved.rows, moved.separate_spaces);
    let revision2 = fixture.handle.update_topology_with(moved, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            20,
            vec![(win(1), vec![11]), (win(2), vec![11]),],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            20,
            vec![(win(9), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(record) = fixture.applied_rx.try_recv() {
        // Generation-gated like the switch test: only frames planned
        // under the switched revision count as hidden writes.
        if (record.2 == "1001_1" || record.2 == "1001_2") && record.4 == revision2 {
            panic!("hidden pair takes no writes while on 12: {record:?}");
        }
    }
    // Close window 2 while hidden (simply absent from its app's next
    // snapshot — actual missing, not mere off-current).
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            30,
            vec![(win(1), vec![11]),],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Return to 11: only the survivor converges, filling the domain alone.
    let mut back = two_display_snapshot();
    back.rows[0].spaces = vec![11, 12];
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision3 = fixture.handle.update_topology_with(back, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            40,
            vec![(win(1), vec![11]),],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            30,
            vec![(win(9), vec![21])],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Converge to the planned survivor frame (deterministic post-barrier
    // truth): superseded half-width records can land late under load.
    let survivor = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .find(|f| f.window == "1001_1")
        .map(|f| (f.x, f.y, f.w, f.h))
        .expect("planned survivor");
    let r1 = wait_applied_to(&fixture, "1001_1", survivor, "sole survivor converges");
    let areas = work_areas();
    assert!(
        within(r1.3, areas[0]),
        "survivor converges on return: {:?}",
        r1.3
    );
    assert_ne!(
        r1.3, f1.3,
        "sole survivor fills the domain (closed sibling never ghosts)"
    );
    let mut desired: Vec<String> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| f.window.clone())
        .collect();
    desired.sort();
    assert_eq!(desired, vec!["1001_1".to_owned(), "1002_9".to_owned()]);
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "close-while-hidden retires nothing"
    );
    shutdown_clean(&fixture);
}

#[test]
fn space_type_gap_retains_hidden_layout_without_release() {
    // A transient unreadable Space type (not a deletion, not a proven
    // non-user Space) must not release its layout: display 1 tiles on 11,
    // switches to 12 while 11 reads unreadable (hidden, silent, retained),
    // then returns to a readable 11 with the exact same frames and no
    // retirement anywhere in between.
    let fixture = start(scoped_config());
    let mut first = two_display_snapshot();
    first.rows[0].spaces = vec![11, 12];
    first.rows[0].current = Some(11);
    first.revision =
        topology::fingerprint_snapshot(&first.displays, &first.rows, first.separate_spaces);
    let revision = fixture.handle.update_topology_with(first, &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11]), (win(2), vec![11]),],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(9), vec![21])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let f1 = wait_applied(&fixture, "1001_1");
    let f2 = wait_applied(&fixture, "1001_2");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Switch to 12 while 11's type reads unreadable: the pair is hidden
    // under the gap (retained, silent), the new window tiles current.
    let mut moved = two_display_snapshot();
    moved.rows[0].spaces = vec![11, 12];
    moved.rows[0].current = Some(12);
    moved.revision =
        topology::fingerprint_snapshot(&moved.displays, &moved.rows, moved.separate_spaces);
    let gap_type = |sid: u64| {
        if sid == 11 { None } else { user_space(sid) }
    };
    let revision2 = fixture.handle.update_topology_with(moved, &gap_type);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            20,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(3), vec![12]),],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            20,
            vec![(win(9), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let c = wait_applied(&fixture, "1001_3");
    assert!(
        within(c.3, work_areas()[0]),
        "switched-to Space tiles d1: {:?}",
        c.3
    );
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(record) = fixture.applied_rx.try_recv() {
        // Generation-gated: only frames planned under the gap revision
        // count; superseded pre-gap records can land late under load.
        if (record.2 == "1001_1" || record.2 == "1001_2") && record.4 == revision2 {
            panic!("unreadable-typed hidden pair takes no writes: {record:?}");
        }
    }
    assert!(
        fixture.focus_rx.try_recv().is_err(),
        "no focus dispatches around a type gap"
    );
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "type gap must not retire the hidden domain"
    );
    // Readable again, back to 11: the exact pre-gap frames return.
    let mut back = two_display_snapshot();
    back.rows[0].spaces = vec![11, 12];
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision3 = fixture.handle.update_topology_with(back, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            30,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(3), vec![12]),],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            30,
            vec![(win(9), vec![21])],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let r1 = wait_applied(&fixture, "1001_1");
    let r2 = wait_applied(&fixture, "1001_2");
    assert_eq!(r1.3, f1.3, "readable return restores exact frames");
    assert_eq!(r2.3, f2.3, "readable return restores exact frames");
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "gap round trip retires nothing"
    );
    shutdown_clean(&fixture);
}

#[test]
fn skipped_display_parks_layout_then_resumes_exact() {
    // Display 1's SLS row goes unhomable (unknown identifier — still
    // present, never absent): its sessions park with no views, no
    // reconciles, and no effects instead of releasing. Fresh evidence for
    // the same keys resumes the exact pre-gap frames with no retirement
    // anywhere.
    let fixture = start(scoped_config());
    let mut first = two_display_snapshot();
    first.rows[0].spaces = vec![11, 12];
    first.rows[0].current = Some(11);
    first.revision =
        topology::fingerprint_snapshot(&first.displays, &first.rows, first.separate_spaces);
    let revision = fixture.handle.update_topology_with(first, &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(4), vec![11]),],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(9), vec![21])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let f1 = wait_applied(&fixture, "1001_1");
    let f2 = wait_applied(&fixture, "1001_2");
    let f4 = wait_applied(&fixture, "1001_4");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Display 1's row identifier turns unknown: display 1 skips (still
    // present, geometry intact) while display 2 homes normally.
    let gap = topology::build_snapshot(
        &[
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ],
        &[
            row(Some("gone-uuid"), &[11, 12], Some(11)),
            row(Some("uuid-2"), &[21], Some(21)),
        ],
        Some(true),
    );
    let revision2 = fixture.handle.update_topology_with(gap, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            20,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(4), vec![11]),],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            20,
            vec![(win(9), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // The skip is recognized (not silent), and the parked triple takes
    // no writes while display 2 keeps tiling alone.
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "topology" && e.outcome == "displays-skipped"),
        "skipped display must be counted"
    );
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(record) = fixture.applied_rx.try_recv() {
        // Generation-gated: only frames planned under the gap revision
        // count; superseded pre-gap records can land late under load.
        if record.2.starts_with("1001_") && record.4 == revision2 {
            panic!("parked display takes no writes: {record:?}");
        }
    }
    let desired: Vec<String> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| f.window.clone())
        .collect();
    assert_eq!(desired, vec!["1002_9".to_owned()]);
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "skipped display must park, never release"
    );
    // Fresh evidence resumes the same keys: the exact pre-gap frames.
    let mut back = two_display_snapshot();
    back.rows[0].spaces = vec![11, 12];
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision3 = fixture.handle.update_topology_with(back, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            30,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(4), vec![11]),],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            30,
            vec![(win(9), vec![21])],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let r1 = wait_applied(&fixture, "1001_1");
    let r2 = wait_applied(&fixture, "1001_2");
    let r4 = wait_applied(&fixture, "1001_4");
    assert_eq!(r1.3, f1.3, "parked session resumes exact frames");
    assert_eq!(r2.3, f2.3, "parked session resumes exact frames");
    assert_eq!(r4.3, f4.3, "parked session resumes exact frames");
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "park round trip retires nothing"
    );
    shutdown_clean(&fixture);
}

#[test]
fn skip_then_actual_removal_releases() {
    // A skipped (still-present) display parks without release; removing
    // it for real afterwards releases its domains normally while the
    // surviving display is undisturbed.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    let revision = fixture.handle.topology_revision().expect("revision");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(2), vec![21])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let _ = wait_applied_pair(&fixture, "1001_1", "1002_2");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Skip display 1 first (unhomable row, still present): parked, never
    // released, no writes into it.
    let gap = topology::build_snapshot(
        &[
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ],
        &[
            row(Some("gone-uuid"), &[11], Some(11)),
            row(Some("uuid-2"), &[21], Some(21)),
        ],
        Some(true),
    );
    let revision2 = fixture.handle.update_topology_with(gap, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            20,
            vec![(win(1), vec![11])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            20,
            vec![(win(2), vec![21])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(record) = fixture.applied_rx.try_recv() {
        // Generation-gated: only frames planned under the gap revision
        // count; superseded pre-gap records can land late under load.
        if record.2 == "1001_1" && record.4 == revision2 {
            panic!("parked display takes no writes: {record:?}");
        }
    }
    assert!(
        !fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "retired"),
        "skip parks without release"
    );
    // Display 1 actually unplugged (absent from geometry and rows, so not
    // skipped): its parked domain releases normally; display 2 tiles on.
    let single = topology::build_snapshot(
        &[ns(2, "uuid-2", 0.0, 0.0, 1440.0, 900.0)],
        &[row(Some("uuid-2"), &[21], Some(21))],
        Some(true),
    );
    let revision3 = fixture.handle.update_topology_with(single, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            30,
            vec![(win(2), vec![21])],
            None,
            revision3
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let b = wait_applied(&fixture, "1002_2");
    assert!(
        within(b.3, (0, 0, 1440, 900)),
        "surviving display keeps tiling: {:?}",
        b.3
    );
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "topology-adopted"),
        "actually-removed display must converge its release"
    );
    let desired: Vec<String> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| f.window.clone())
        .collect();
    assert_eq!(desired, vec!["1002_2".to_owned()]);
    shutdown_clean(&fixture);
}

#[test]
fn removed_display_retires_its_domains() {
    // Two displays tile; removing display 2 retires exactly its domain
    // while display 1 is undisturbed.
    let fixture = start(scoped_config());
    fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    // Pin generations before the first revision fence below.
    let revision = fixture.handle.topology_revision().expect("revision");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(2), vec![21])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let _ = wait_applied_pair(&fixture, "1001_1", "1002_2");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Display 2 unplugged: single display, single row.
    let single = topology::build_snapshot(
        &[ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)],
        &[row(Some("uuid-1"), &[11], Some(11))],
        Some(true),
    );
    let revision2 = fixture.handle.update_topology_with(single, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            20,
            vec![(win(1), vec![11])],
            None,
            revision2
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let a = wait_applied(&fixture, "1001_1");
    assert!(
        within(a.3, work_areas()[0]),
        "surviving display keeps tiling: {:?}",
        a.3
    );
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "domain" && e.outcome == "topology-adopted"),
        "removed display must converge its removal"
    );
    let desired: Vec<String> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| f.window.clone())
        .collect();
    assert_eq!(desired, vec!["1001_1".to_owned()]);
    shutdown_clean(&fixture);
}

#[test]
fn unknown_membership_never_adopts_current() {
    // Unknown (empty), sticky (multi-Space), and foreign-Space windows
    // never adopt the current domain even when their geometry sits inside
    // its work area; only the exactly-homed member actuates.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_display_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![
                (win_at(1, 100, 100, 200, 200), vec![]),
                (win_at(2, 100, 100, 200, 200), vec![11, 21]),
                (win_at(3, 100, 100, 200, 200), vec![99]),
                (win_at(4, 100, 100, 200, 200), vec![11]),
            ],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let applied = wait_applied(&fixture, "1001_4");
    assert_eq!(applied.2, "1001_4");
    assert!(
        fixture.applied_rx.try_recv().is_err(),
        "unhomed windows must never actuate"
    );
    let desired = fixture.handle.last_desired_geometry();
    assert_eq!(
        desired
            .iter()
            .map(|f| f.window.as_str())
            .collect::<Vec<_>>(),
        vec!["1001_4"]
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
        cg_complete: true,
        cg_owner_wids: vec![1],
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
                win_at(1, 8, 8, 466, 884),
                win_at(2, 482, 8, 466, 884),
                win_at(3, 956, 8, 468, 884),
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
                win_at(1, 8, 8, 466, 884),
                win_at(2, 482, 8, 466, 884),
                win_at(3, 956, 8, 468, 884),
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

// -- Select-only product arming --------------------------------------------------

/// Product tap table for select tests: the basic chords plus the 18 armed
/// selects under Authentic/Ctrl.
fn select_product_bindings() -> Vec<tiler_macos::runtime::TileBinding> {
    let mut catalogue = tiler_macos::native_input::tile_catalogue();
    catalogue.extend(tiler_macos::workspace_select::bindings());
    tiler_macos::input_policy::effective_bindings(
        tiler_macos::input_policy::InputPreset::Authentic,
        ProbeModifier::Ctrl,
        &catalogue,
    )
}

fn select_config(bindings: Vec<tiler_macos::runtime::TileBinding>) -> RuntimeConfig {
    RuntimeConfig {
        topology_mode: TopologyMode::Native,
        tile_bindings: Some(bindings),
        ..RuntimeConfig::default()
    }
}

/// One display with two user Spaces, 11 current (12 empty): the arrival
/// target for the no-focus select legs below.
fn two_space_snapshot() -> topology::TopologySnapshot {
    topology::build_snapshot(
        &[ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)],
        &[row(Some("uuid-1"), &[11, 12], Some(11))],
        Some(true),
    )
}

fn send_select_chord(
    fixture: &Fixture,
    bindings: &[tiler_macos::runtime::TileBinding],
    action: tiler_macos::runtime::InputSemanticAction,
) {
    let binding = bindings
        .iter()
        .find(|b| b.action == action)
        .expect("select chord armed");
    let flags = tiler_macos::workspace_input::effective_flags_for(ProbeModifier::Ctrl, binding);
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: binding.key_code,
        })
        .expect("send select chord");
}

fn log_has(handle: &RuntimeHandle, kind: &str, outcome: &str) -> bool {
    handle
        .log_snapshot()
        .iter()
        .any(|e| e.kind == kind && e.outcome == outcome)
}

#[test]
fn select_enqueues_without_focus_and_serializes_concurrent_presses() {
    // No apps, no observations, no focused window anywhere: an armed
    // numbered select still enqueues for the main-thread drain (empty
    // Spaces navigate), without contacting the core; a second press while
    // claimed refuses busy instead of overlapping — never replayed.
    use tiler_macos::runtime::InputSemanticAction as Action;
    let bindings = select_product_bindings();
    assert_eq!(bindings.len(), 53, "34 basic + 19 armed selects");
    let fixture = start(select_config(bindings.clone()));
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    send_select_chord(&fixture, &bindings, Action::WorkspaceSelect { index: 2 });
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_enqueued == 1,
        "no-focus select enqueued",
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics.input_forwarded, 0,
        "selects never reach the core"
    );
    assert_eq!(diagnostics.input_select_refused, 0);
    assert!(log_has(&fixture.handle, "input", "select-enqueued"));
    // Concurrent press while the claim is held: refused busy, first claim kept.
    send_select_chord(
        &fixture,
        &bindings,
        Action::WorkspaceRelative { prev: false },
    );
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_refused == 1,
        "concurrent select refused busy",
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(diagnostics.input_select_enqueued, 1, "no second enqueue");
    assert_eq!(diagnostics.input_forwarded, 0);
    assert!(log_has(&fixture.handle, "input", "refused-select-busy"));
    shutdown_clean(&fixture);
}

#[test]
fn unarmed_workspace_rows_refuse_without_dispatch() {
    // Test-only over-arming (the product never arms these): relative send
    // and output send still refuse engine-side with their honest tokens
    // and never enqueue - the engine is the backstop. Trailing select-0
    // and send-0 follow are armed in product and dispatch (covered by the
    // send-claim test below for sends and the select unit lanes for
    // select-0), so they stay out of this unarmed set. Numbered sends
    // 0..9 follow plus 1..9 stay are armed in
    // product and dispatch (covered by the send-claim test below). No
    // planner dispatch runs for the unsupported rows: no claim enqueues,
    // no world command is forwarded.
    use tiler_macos::runtime::InputDirection;
    use tiler_macos::runtime::InputSemanticAction as Action;
    let mut bindings = select_product_bindings();
    bindings.extend(
        tiler_macos::workspace_input::workspace_catalogue()
            .into_iter()
            .filter(|row| matches!(row.id, "send-prev-h" | "send-output-left"))
            .filter_map(|row| row.binding()),
    );
    // Product arms numbered send-follow 0..9 (not over-armed here).
    assert_eq!(tiler_macos::workspace_send::bindings().len(), 10);
    let fixture = start(select_config(bindings.clone()));
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    for action in [
        Action::WorkspaceSendRelative {
            prev: true,
            follow: true,
        },
        Action::WorkspaceOutput {
            direction: InputDirection::Left,
            follow: true,
        },
    ] {
        send_select_chord(&fixture, &bindings, action.clone());
    }
    // Stay rows are bindable-unbound (no chord): trailing stay-0 refuses
    // via direct submit (numbered stays 1..9 are armed and dispatch, so
    // they stay out of this unarmed set).
    assert_eq!(
        fixture.handle.submit_input_action(Action::WorkspaceSend {
            index: 0,
            follow: false
        }),
        SubmitOutcome::Accepted
    );
    wait_diagnostics(
        &fixture.handle,
        |d| {
            d.input_refused_unsupported == 3
                && log_has(
                    &fixture.handle,
                    "input",
                    "refused-workspace-send-unsupported",
                )
                && log_has(
                    &fixture.handle,
                    "input",
                    "refused-workspace-output-unsupported",
                )
        },
        "unsupported sends refuse",
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    assert_eq!(
        diagnostics.input_select_enqueued, 0,
        "unarmed rows never enqueue"
    );
    assert_eq!(diagnostics.input_forwarded, 0);
    assert_eq!(diagnostics.input_select_refused, 0);
    assert!(
        !log_has(&fixture.handle, "input", "send-enqueued"),
        "no send claim enqueues"
    );
    assert!(
        !log_has(&fixture.handle, "input", "select-enqueued"),
        "no select claim enqueues"
    );
    assert!(
        !log_has(&fixture.handle, "core-reply", "send-accepted")
            && !log_has(&fixture.handle, "core-reply", "send-consumed"),
        "no planner send dispatch for unsupported sends"
    );
    shutdown_clean(&fixture);
}

#[test]
fn numbered_send_hotkey_claim_settles_transfer_end_to_end() {
    // Producer-shaped pathway with no live touch: the armed Ctrl+Shift+2
    // hotkey admits engine-side (managed subject, lifetime, source, plus
    // the reply-bound target plan and operation identity), claims for the
    // main-thread drain, and the reported outcome stages transfer feedback
    // for the next world report — so the core settles the retained
    // transfer. The dedicated effect owner proves the same pins over
    // injected readers (see `workspace_send_live_tests`); here the engine
    // half plus the feedback half run through the real runtime.
    use tiler_core::world_report::FrameSetter;
    use tiler_macos::runtime::InputSemanticAction as Action;
    let mut bindings = select_product_bindings();
    bindings.extend(tiler_macos::workspace_send::bindings());
    assert_eq!(bindings.len(), 63, "34 basic + 19 selects + 10 sends");
    let fixture = start(select_config(bindings.clone()));
    let revision = fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            1,
            vec![(win_at(1, 100, 100, 200, 200), vec![11])],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    send_select_chord(
        &fixture,
        &bindings,
        Action::WorkspaceSend {
            index: 2,
            follow: true,
        },
    );
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_enqueued == 1,
        "armed send enqueued",
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(log_has(&fixture.handle, "input", "send-enqueued"));
    let claimed = fixture.handle.claim_select_for_drain().expect("send claim");
    assert_eq!(
        claimed.action,
        Action::WorkspaceSend {
            index: 2,
            follow: true
        }
    );
    assert_eq!(claimed.send_subject.as_deref(), Some("1001_1"));
    assert_eq!(claimed.send_generation, Some(gen_a));
    assert_eq!(claimed.topology_revision, Some(revision));
    assert_eq!(
        claimed.expected_sid,
        claimed
            .select_native_binding
            .as_deref()
            .and_then(|binding| binding.strip_prefix("space-")?.parse::<u64>().ok()),
        "send target SID rides the reply binding, never an input-ordinal pin"
    );
    assert!(claimed.expected_sid.is_some(), "target SID bound");
    assert!(claimed.send_source.is_some(), "source domain pinned");
    assert!(
        claimed.select_target_output.is_some() && claimed.select_target_workspace.is_some(),
        "reply plan binds the retained transfer target"
    );
    let op = claimed.operation_id.clone().expect("bound operation id");
    let baseline = claimed.select_baseline_sid;
    let follow_view = claimed.send_follow_view_operation.clone();
    fixture.handle.release_select_after_drain();
    // Offline no-touch pathway: no native write fired, so the raw setter is
    // `Refused` with no reads (holds without actuation — never a verdict).
    fixture.handle.report_send_outcome(
        &op,
        FrameSetter::Refused,
        None,
        None,
        None,
        None,
        baseline,
        follow_view.as_deref(),
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(
        fixture
            .handle
            .log_snapshot()
            .iter()
            .any(|e| e.kind == "feedback" && e.outcome == "staged"),
        "send outcome must stage transfer feedback"
    );
    shutdown_clean(&fixture);
}

#[test]
fn select_refuses_without_published_topology() {
    use tiler_macos::runtime::InputSemanticAction as Action;
    let bindings = select_product_bindings();
    let fixture = start(select_config(bindings.clone()));
    // No topology published: refuse instead of resolving against nothing.
    send_select_chord(&fixture, &bindings, Action::WorkspaceSelect { index: 2 });
    wait_diagnostics(
        &fixture.handle,
        |d| {
            d.input_select_refused == 1
                && log_has(&fixture.handle, "input", "refused-select-no-topology")
        },
        "select without topology refused",
    );
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(fixture.handle.diagnostics().input_select_enqueued, 0);
    shutdown_clean(&fixture);
}

#[test]
fn arrival_on_empty_space_retains_hidden_layout_without_writes() {
    // Display 1 knows Spaces 11+12; one window tiles on 11. Simulating the
    // post-select arrival (current flips to the empty 12): the hidden
    // window takes no frames and no focus, its session survives, and
    // switching back resumes the exact layout — selection stays usable
    // after arrival on an empty target Space.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let first = wait_applied(&fixture, "1001_1");
    while fixture.applied_rx.try_recv().is_ok() {}
    // Arrival: current flips to the empty 12; the window stays homed on
    // hidden 11 and is simply absent from the current view.
    let mut arrived = two_space_snapshot();
    arrived.rows[0].current = Some(12);
    arrived.revision =
        topology::fingerprint_snapshot(&arrived.displays, &arrived.rows, arrived.separate_spaces);
    let revision = fixture.handle.update_topology_with(arrived, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            11,
            vec![(win(1), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(
        fixture
            .applied_rx
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "hidden window takes no writes on the empty arrival Space"
    );
    // Return trip: the retained layout resumes exactly.
    let mut back = two_space_snapshot();
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision = fixture.handle.update_topology_with(back, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            12,
            vec![(win(1), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let resumed = wait_applied_to(&fixture, "1001_1", first.3, "return trip");
    assert_eq!(
        resumed.3, first.3,
        "hidden session survives the empty arrival"
    );
    shutdown_clean(&fixture);
}

#[test]
fn hidden_space_roundtrip_preserves_per_window_slots_across_blind_interval() {
    // Producer-shaped native full-world Space roundtrip (live 2026-10-18
    // leg r4): three app-A windows plus one helper tile on Space 11 with
    // the helper focused; outward arrival on the empty Space 12 blinds
    // live AX for the hidden apps (successful but EMPTY snapshots — live
    // published/applied count=0 with zero observation failures); a new
    // helper admitted on 12 while it is current; return to 11 with AX
    // re-listing the Space-11 windows in reverse enumeration order. The
    // retained per-Space layout must resume per window identity — the
    // live 3684<->3686 slot rotation with 3685 fixed is the fresh-reseed
    // fingerprint and must not recur.
    //
    // Oracles are causal, never settle-sleeps: the settled oracle is the
    // core's exact desired plan (`last_desired_geometry`, written
    // synchronously on the engine thread, so one flush fences it); the
    // hidden legs are pinned by the retained-occupancy control query
    // (strict FIFO with observations) plus the cumulative `frames_emitted`
    // counter (zero hidden dispatches); the return leg collects every
    // applied record under the file's usual WAIT deadline and forbids any
    // intermediate reassignment.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch A");
    let gen_b = fixture.handle.launch_app(APP_B).expect("launch B");
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11]), (win(2), vec![11]), (win(3), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            10,
            vec![(win(1), vec![11])],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // Settled oracle: the core's exact desired plan, fenced by the flush
    // above (the plan is written synchronously on the engine thread, so no
    // quiescence wait is needed to read it).
    let mut settled: BTreeMap<String, (i32, i32, i32, i32)> = BTreeMap::new();
    for row in fixture.handle.last_desired_geometry() {
        settled.insert(row.window.clone(), (row.x, row.y, row.w, row.h));
    }
    assert_eq!(
        settled.keys().cloned().collect::<Vec<_>>(),
        vec!["1001_1", "1001_2", "1001_3", "1002_1"],
        "settled plan must tile exactly the four admitted windows"
    );
    let at = |window: &str, wid: u32| {
        let rect = settled[window];
        (win_at(wid, rect.0, rect.1, rect.2, rect.3), vec![11])
    };
    // Converged readback: live AX re-reads the tiled frames, so the
    // return-leg snapshots below carry settled geometry exactly like a
    // live readback (observed rectangles never steer the core plan, but
    // the shape matches production, where AX reports real frames).
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            11,
            vec![at("1001_1", 1), at("1001_2", 2), at("1001_3", 3)],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            11,
            vec![at("1002_1", 1)],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // No drain needed here: pre-return leftovers all carry settled rects,
    // and the return collector below attributes records by per-app command
    // id, so stragglers can neither false-fail nor satisfy convergence.
    // Outward arrival on the empty 12: blind (empty-member) snapshots for
    // the hidden apps, twice, mirroring the live repeated empties.
    let mut arrived = two_space_snapshot();
    arrived.rows[0].current = Some(12);
    arrived.revision =
        topology::fingerprint_snapshot(&arrived.displays, &arrived.rows, arrived.separate_spaces);
    let revision = fixture.handle.update_topology_with(arrived, &user_space);
    let emitted_before = fixture.handle.diagnostics().frames_emitted;
    for seq in [12, 13] {
        assert_eq!(
            fixture.handle.submit_scoped_observation(scoped_obs(
                APP_A,
                gen_a,
                seq,
                vec![],
                None,
                revision
            )),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            fixture.handle.submit_scoped_observation(scoped_obs(
                APP_B,
                gen_b,
                seq,
                vec![],
                None,
                revision
            )),
            SubmitOutcome::Accepted
        );
        assert!(fixture.handle.flush(FLUSH));
    }
    // Hidden legs dispatch nothing: non-current rows are filtered before
    // emission, so the cumulative counter must not advance.
    assert_eq!(
        fixture.handle.diagnostics().frames_emitted,
        emitted_before,
        "hidden snapshots must dispatch no frames"
    );
    // Retained membership must survive the blind interval: the occupancy
    // control query is strict FIFO with observations, so it reflects every
    // blind submit above.
    let occupied = fixture
        .handle
        .query_workspace_occupancy(WAIT)
        .expect("occupancy query must answer");
    let hidden = occupied
        .domains
        .iter()
        .find(|d| !d.current && d.members.contains(&(1001, 1)))
        .expect("Space-11 homing must survive the blind interval")
        .members
        .clone();
    assert_eq!(
        hidden,
        vec![(1001, 1), (1001, 2), (1001, 3), (1002, 1)],
        "blind snapshots must preserve every hidden member"
    );
    // Independent domain: a new helper admitted on the current 12 tiles
    // there without touching the hidden Space-11 layout.
    let gen_c = fixture.handle.launch_app(APP_C).expect("launch C");
    let emitted_before = fixture.handle.diagnostics().frames_emitted;
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_C,
            gen_c,
            10,
            vec![(win(1), vec![12])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let c_first = wait_applied(&fixture, "1003_1");
    assert!(
        within(c_first.3, work_areas()[0]),
        "new helper must tile on the current Space, got {:?}",
        c_first.3
    );
    // Exactly one frame dispatched this leg — the helper's own — so no
    // hidden Space-11 row actuated.
    assert_eq!(
        fixture.handle.diagnostics().frames_emitted,
        emitted_before + 1,
        "admission on 12 must dispatch only the new helper frame"
    );
    // Retained occupancy still homes every hidden Space-11 member.
    let occupied = fixture
        .handle
        .query_workspace_occupancy(WAIT)
        .expect("occupancy query must answer");
    let hidden = occupied
        .domains
        .iter()
        .find(|d| !d.current && d.members.contains(&(1001, 1)))
        .expect("Space-11 homing must survive the 12 admission")
        .members
        .clone();
    assert_eq!(
        hidden,
        vec![(1001, 1), (1001, 2), (1001, 3), (1002, 1)],
        "admission on 12 must preserve every hidden member"
    );
    // Return trip to 11: AX re-lists the Space-11 windows in reverse
    // enumeration order (the live re-listing order hypothesis behind the
    // outer-swap fingerprint); the helper on 12 goes blind in turn.
    // True focus is unchanged throughout: no focus commands are sent.
    //
    // Record attribution: per-app command ids rise monotonically at
    // dispatch, so the best-effort drain below snapshots the pre-return
    // high-water mark per app; only records above their app's mark count
    // as return-leg evidence. Older settled-rect stragglers can neither
    // false-fail nor satisfy convergence. (Completeness of this drain is
    // not load-bearing: any straggler it misses still carries a settled
    // rect, and convergence requires post-mark evidence per window.)
    let mut watermarks: BTreeMap<u32, u64> = BTreeMap::new();
    while let Ok(record) = fixture.applied_rx.try_recv() {
        watermarks
            .entry(record.0)
            .and_modify(|mark| *mark = (*mark).max(record.1))
            .or_insert(record.1);
    }
    let mut back = two_space_snapshot();
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision = fixture.handle.update_topology_with(back, &user_space);
    let emitted_before = fixture.handle.diagnostics().frames_emitted;
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            14,
            vec![at("1001_3", 3), at("1001_2", 2), at("1001_1", 1)],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_B,
            gen_b,
            14,
            vec![at("1002_1", 1)],
            Some(1),
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_C,
            gen_c,
            11,
            vec![],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    // The return leg must dispatch (guards vacuity-by-straggler: without
    // fresh dispatches the collector below could converge on pre-return
    // records alone).
    let emitted_after = fixture.handle.diagnostics().frames_emitted;
    assert!(
        emitted_after > emitted_before,
        "return observations must dispatch the resumed layout"
    );
    // Collect every applied record until each settled window has
    // post-watermark evidence (WAIT-deadline recv loop, the same idiom as
    // `wait_applied_to` — except nothing is skipped: convergence counts
    // any post-mark record per window, and every record is retained, so a
    // permuted frame fails the identity check below instead of being
    // silently passed over while waiting for the right rect).
    let deadline = std::time::Instant::now() + WAIT;
    let mut returned: Vec<AppliedRecord> = Vec::new();
    let mut converged: BTreeSet<String> = BTreeSet::new();
    while converged.len() < settled.len() {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "return trip never actuated the settled windows (converged {converged:?}, collected {returned:?}, settled {settled:?})"
        );
        match fixture.applied_rx.recv_timeout(remaining) {
            Ok(record) => {
                let fresh = record.1 > watermarks.get(&record.0).copied().unwrap_or(0);
                if fresh {
                    converged.insert(record.2.clone());
                }
                returned.push(record);
            }
            Err(_) => panic!(
                "return trip never actuated the settled windows (converged {converged:?}, collected {returned:?}, settled {settled:?})"
            ),
        }
    }
    // Best-effort tail drain: records already delivered behind the
    // convergence point are evidence too.
    while let Ok(record) = fixture.applied_rx.try_recv() {
        returned.push(record);
    }
    // Non-vacuity: exactly the four settled windows wrote — no more, no
    // fewer (convergence above already implies coverage; this forbids
    // strays such as a helper rewrite).
    let wrote: BTreeSet<String> = returned.iter().map(|record| record.2.clone()).collect();
    assert_eq!(
        wrote,
        settled.keys().cloned().collect::<BTreeSet<_>>(),
        "return trip must write exactly the settled windows (collected {returned:?})"
    );
    // Identity stability: every return-leg frame carries its settled
    // rect — any slot reassignment, intermediate or final, fails here.
    // (Offline the worker cannot redundant-skip — its last-scoped
    // readback only converges via backend reads, which direct submits
    // bypass — so same-rect rewrites still arrive; live they are
    // suppressed once AX reads the frames back. The invariant under test
    // is identity stability, which same-rect rewrites preserve.)
    let mut mismatched: Vec<String> = Vec::new();
    for record in &returned {
        let expected = &settled[&record.2];
        if &record.3 != expected {
            mismatched.push(format!(
                "{w}: wrote {rect:?}, settled {expected:?}",
                w = record.2,
                rect = record.3
            ));
        }
    }
    assert!(
        mismatched.is_empty(),
        "return trip must never reassign window->slot identities: {mismatched:?} (settled {settled:?})"
    );
    shutdown_clean(&fixture);
}

#[test]
fn blind_hidden_observation_keeps_current_loss_authoritative_and_drops_retired_homing() {
    // Fences around blind-hidden preservation (see
    // `handle_scoped_observation`): an empty snapshot of a CURRENT domain
    // stays authoritative loss, and homing to a retired Space is dropped
    // instead of preserved. Oracles are causal throughout:
    // engine-flush-fenced submits plus the strict-FIFO occupancy control
    // query; no applied frames and no sleeps anywhere in this test.
    let fixture = start(scoped_config());
    let revision = fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    let gen_a = fixture.handle.launch_app(APP_A).expect("launch A");
    let occupied = || {
        fixture
            .handle
            .query_workspace_occupancy(WAIT)
            .expect("occupancy query must answer")
    };
    let lists = |report: &tiler_macos::runtime::WorkspaceOccupancyReport, member: (u32, u32)| {
        report.domains.iter().any(|d| d.members.contains(&member))
    };
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            10,
            vec![(win(1), vec![11])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(lists(&occupied(), (1001, 1)), "setup must home the window");
    // Current-empty stays authoritative: the emptied current Space
    // converges to empty.
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            11,
            vec![],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(
        !lists(&occupied(), (1001, 1)),
        "emptied current Space must converge to empty"
    );
    // Hide the window on 12, then blind it: preservation holds while the
    // domain is still known.
    let mut out = two_space_snapshot();
    out.rows[0].current = Some(12);
    out.revision = topology::fingerprint_snapshot(&out.displays, &out.rows, out.separate_spaces);
    let revision = fixture.handle.update_topology_with(out, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            12,
            vec![(win(1), vec![12])],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(
        lists(&occupied(), (1001, 1)),
        "re-admission on 12 must home the window"
    );
    let mut back = two_space_snapshot();
    back.rows[0].current = Some(11);
    back.revision =
        topology::fingerprint_snapshot(&back.displays, &back.rows, back.separate_spaces);
    let revision = fixture.handle.update_topology_with(back, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            13,
            vec![],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    let domain = occupied()
        .domains
        .into_iter()
        .find(|d| d.members.contains(&(1001, 1)))
        .expect("hidden homing must survive while the Space is known");
    assert!(
        !domain.current,
        "the surviving homing must be the hidden domain"
    );
    // Retire 12: preservation must not resurrect homing to a dead Space.
    let mut retired = two_space_snapshot();
    retired.rows[0].spaces = vec![11];
    retired.rows[0].current = Some(11);
    retired.revision =
        topology::fingerprint_snapshot(&retired.displays, &retired.rows, retired.separate_spaces);
    let revision = fixture.handle.update_topology_with(retired, &user_space);
    assert_eq!(
        fixture.handle.submit_scoped_observation(scoped_obs(
            APP_A,
            gen_a,
            14,
            vec![],
            None,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(
        !lists(&occupied(), (1001, 1)),
        "retired Space homing must drop"
    );
    shutdown_clean(&fixture);
}

#[test]
fn select_claim_release_discard_protocol() {
    // The claim slot serializes the tap/engine/main handoff offline: an
    // empty slot claims/drops nothing; a claim takes the pending action
    // and blocks a second claim while held (concurrent presses refuse
    // busy); release frees the claim with nothing left pending; discard
    // drops a fresh pending without dispatch and never touches a drain
    // in progress. No live tap, no Spaces, no desktop effect.
    use tiler_macos::runtime::InputSemanticAction as Action;
    let bindings = select_product_bindings();
    let fixture = start(select_config(bindings.clone()));
    assert!(fixture.handle.claim_select_for_drain().is_none());
    assert!(!fixture.handle.discard_select_pending());
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    send_select_chord(&fixture, &bindings, Action::WorkspaceSelect { index: 2 });
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_enqueued == 1,
        "select enqueued for claim",
    );
    assert!(fixture.handle.flush(FLUSH));
    let claimed = fixture
        .handle
        .claim_select_for_drain()
        .expect("pending claim");
    assert_eq!(claimed.action, Action::WorkspaceSelect { index: 2 });
    // Ordinary taps bind no pre-reply SID: targeting resolves core-side
    // and the command carries no output (cursor/focus resolves it).
    assert_eq!(
        claimed.expected_sid, None,
        "numbered tap binds no admission SID"
    );
    assert_eq!(claimed.ui_generation, 0);
    assert_eq!(
        claimed.topology_revision, None,
        "numbered tap pins no revision; the drain fences nothing pre-reply"
    );
    assert_eq!(
        claimed.operation_id.as_deref(),
        Some("op-1"),
        "claim binds the world operation identity for drain feedback"
    );
    // The accepted reply binds the plan: retained logical target plus the
    // live native binding with the observation-time baseline.
    assert_eq!(
        claimed.select_target_output.as_deref(),
        Some("macos-display-1")
    );
    assert_eq!(
        claimed.select_native_binding.as_deref(),
        Some("space-12"),
        "numbered tap actuates the live binding, never a pinned ordinal"
    );
    assert_eq!(claimed.select_baseline_sid, Some(11));
    assert!(fixture.handle.claim_select_for_drain().is_none());
    assert!(
        !fixture.handle.discard_select_pending(),
        "discard never touches a drain in progress"
    );
    fixture.handle.release_select_after_drain();
    assert!(fixture.handle.claim_select_for_drain().is_none());
    send_select_chord(&fixture, &bindings, Action::WorkspacePrevious);
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_enqueued == 2,
        "second select enqueued",
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(fixture.handle.discard_select_pending());
    assert!(!fixture.handle.discard_select_pending());
    assert!(fixture.handle.claim_select_for_drain().is_none());
    shutdown_clean(&fixture);
}

#[test]
fn select_drain_and_discard_outcomes_stage_feedback_end_to_end() {
    // Proven drain/discard outcomes travel the full honest path from the
    // public handoff to a staged engine row: the drain reports its bound
    // operation outcome, a discard stages proven refusal for its bound
    // claim, and both land ring-observable as staged feedback for the
    // next world report — so the core settles the planner-owned view
    // target instead of retaining it. No live tap, no Spaces.
    use tiler_core::world_report::FrameSetter;
    use tiler_macos::runtime::InputSemanticAction as Action;
    let bindings = select_product_bindings();
    let fixture = start(select_config(bindings.clone()));
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
    let staged_count = || {
        fixture
            .handle
            .log_snapshot()
            .iter()
            .filter(|e| e.kind == "feedback" && e.outcome == "staged")
            .count()
    };
    // Drain path: admitted select, claimed, released, outcome reported.
    // The chord travels the async input thread, so the diagnostics wait
    // (existing helper) proves engine admission before the flush barrier
    // fences the rest.
    send_select_chord(&fixture, &bindings, Action::WorkspaceSelect { index: 2 });
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_enqueued == 1,
        "select enqueued for claim",
    );
    assert!(fixture.handle.flush(FLUSH));
    let claimed = fixture.handle.claim_select_for_drain().expect("claim");
    let op = claimed.operation_id.clone().expect("bound operation id");
    fixture.handle.release_select_after_drain();
    fixture
        .handle
        .report_select_attempt(&op, FrameSetter::Accepted, None, None, None);
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(staged_count(), 1, "drain outcome must stage exactly once");
    // Discard path: a second pending claim dropped without dispatch still
    // stages its proven refusal (unbound claims stage nothing).
    send_select_chord(&fixture, &bindings, Action::WorkspacePrevious);
    wait_diagnostics(
        &fixture.handle,
        |d| d.input_select_enqueued == 2,
        "second select enqueued",
    );
    assert!(fixture.handle.flush(FLUSH));
    assert!(fixture.handle.discard_select_pending());
    assert!(fixture.handle.flush(FLUSH));
    assert_eq!(
        staged_count(),
        2,
        "discarded bound claim must stage its refusal"
    );
    shutdown_clean(&fixture);
}

#[test]
fn stale_select_flight_still_binds_claim_and_never_replays() {
    // Stale command reply refused with no stale emission; the pending
    // claim still binds under its operation identity exactly once (the
    // honest observation-only retry recovers under the same identity with
    // a current reply - never a replay of the consumed command), and the
    // racing select refuses busy instead of overlapping. Channel
    // rendezvous only.
    use std::sync::atomic::{AtomicBool, Ordering};
    use tiler_macos::runtime::{InputSemanticAction as Action, MidflightHook};
    let bindings = select_product_bindings();
    let fixture = start(select_config(bindings.clone()));
    fixture
        .handle
        .update_topology_with(two_space_snapshot(), &user_space);
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
    fixture.handle.set_midflight_hook_for_test(Some(hook));
    // First select opens its flight and parks mid-flight.
    send_select_chord(&fixture, &bindings, Action::WorkspaceSelect { index: 2 });
    flight_open_rx
        .recv_timeout(WAIT)
        .expect("select flight must open");
    // Racing select arrives while the request is outstanding: it marks
    // the watermark past the flight (and later refuses busy — never a
    // second dispatch of the consumed command).
    assert_eq!(
        fixture
            .handle
            .submit_input_action(Action::WorkspaceSelect { index: 3 }),
        SubmitOutcome::Accepted
    );
    proceed_tx.send(()).expect("engine parked mid-flight");
    assert!(fixture.handle.flush(FLUSH));
    fixture.handle.set_midflight_hook_for_test(None);
    // Stale command reply refused with no stale emission; the claim bound
    // with its operation identity exactly once; the racer refused busy.
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics
            .core_replies
            .get("select-refused-stale-observation")
            .copied()
            .unwrap_or(0)
            >= 1,
        "stale select reply must refuse: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(diagnostics.input_select_enqueued, 1);
    assert!(
        diagnostics.input_select_refused >= 1,
        "racing select must refuse busy: {diagnostics:?}"
    );
    let claimed = fixture.handle.claim_select_for_drain().expect("claim");
    assert_eq!(
        claimed.operation_id.as_deref(),
        Some("op-1"),
        "stale select still binds its operation identity"
    );
    // The recovered retry reply is current, so the bound plan resolves
    // against live truth: the binding names a SID in the current native
    // row - never a stale occupant, never a replay.
    assert_eq!(
        claimed.select_native_binding.as_deref(),
        Some("space-12"),
        "recovered plan binds the live native, not the stale occupant"
    );
    assert_eq!(claimed.select_baseline_sid, Some(11));
    fixture.handle.release_select_after_drain();
    shutdown_clean(&fixture);
}
