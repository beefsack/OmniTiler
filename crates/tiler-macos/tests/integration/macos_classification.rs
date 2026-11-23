//! macOS classifier → fake engine observation integration (offline, portable).
//!
//! Deterministic fakes drive the runtime through its public typed handoffs;
//! every test classifies typed AX/CG/app facts through
//! [`tiler_macos::classifier`] and submits the rendered observations over the
//! existing fake engine path (`submit_observation` + `flush`), asserting
//! actual core outputs (desired geometry, applied frames, counted outcomes).
//! No live AX, no taps, no Spaces, no desktop effects.
//!
//! Fixture contract (mirrors `runtime_phase1.rs`): the fake backend's
//! `snapshot()` always fails honestly, so the worker never publishes and the
//! test owns the whole per-app sequence space starting at 1. Positive waits
//! ride the fake's applied-record condvar (the record itself is the proof,
//! no polling); negative assertions first fence causally with
//! `wait_app_settled_for_test` (worker drain-through plus engine tails for
//! every relevant batch) and then assert immediate absence: nothing
//! outstanding can arrive late, so no quiet-time poll is needed.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use tiler_macos::classifier::{
    ActivationPolicy, AppFacts, AxFacts, CgFacts, ClassifiedWindow, Disposition, ExcludeReason,
    FloatReason, ROLE_SHEET, ROLE_WINDOW, SUBROLE_DIALOG, SUBROLE_FLOATING,
    SUBROLE_STANDARD, WindowFrame, classify_window,
};
use tiler_macos::ffi::{DisplayRect, NsDisplay, SlsDisplayRow};
use tiler_macos::runtime::{
    AxCompletion, FrameCommand, ObservedWindow, Runtime, RuntimeConfig, RuntimeHandle,
    ScopedBackendSnapshot, ScopedObservation, ScopedWindowMeta, ShutdownOutcome, SubmitOutcome,
    TopologyMode, WindowSizeHints,
};
use tiler_macos::runtime_types::ExcludedOmission;
use tiler_macos::runtime_backend::{AxBackend, AxBackendFactory, EffectHook, NativeInputStub};
use tiler_macos::topology;

const APP_A: u32 = 1001;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

// -- Lean fakes ---------------------------------------------------------------

#[derive(Debug, Clone)]
struct Applied {
    window: String,
    cmd_id: u64,
    rect: (i32, i32, i32, i32),
}

#[derive(Default)]
struct FakeState {
    applied: Vec<Applied>,
    /// Autonomous scoped observation inboxes, one per pid, installed before
    /// launch and taken by the owning worker at creation (the production
    /// backend route: classifier exclusions and minimized facts ride the
    /// same snapshot).
    inboxes: BTreeMap<u32, mpsc::Receiver<Result<ScopedBackendSnapshot, String>>>,
    /// Worker-bound (current) topology revision, stamped on every polled
    /// snapshot so the engine's revision fence matches.
    bound: Option<u64>,
}

struct FakeFactory {
    state: Arc<(Mutex<FakeState>, Condvar)>,
}

impl AxBackendFactory for FakeFactory {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String> {
        let poll_rx = self
            .state
            .0
            .lock()
            .expect("mutex")
            .inboxes
            .remove(&pid);
        Ok(Box::new(FakeBackend {
            state: Arc::clone(&self.state),
            poll_rx,
        }))
    }
}

struct FakeBackend {
    state: Arc<(Mutex<FakeState>, Condvar)>,
    poll_rx: Option<mpsc::Receiver<Result<ScopedBackendSnapshot, String>>>,
}

impl AxBackend for FakeBackend {
    fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion {
        let (lock, wake) = &*self.state;
        lock.lock().expect("mutex").applied.push(Applied {
            window: cmd.window.clone(),
            cmd_id: cmd.cmd_id,
            rect: (cmd.x, cmd.y, cmd.w, cmd.h),
        });
        // The record itself wakes positive waiters: no polling, no sleep.
        wake.notify_all();
        AxCompletion::Applied { elapsed_ms: 0 }
    }

    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
        // Honest failure keeps the manual sequence space exclusive to the
        // test (see module docs); retained engine state is preserved.
        Err("fake snapshot not configured".to_owned())
    }

    fn bind_effect_hook(&mut self, hook: &EffectHook) {
        let revision = hook.cell.load().map(|ctx| ctx.revision);
        let (lock, _) = &*self.state;
        lock.lock().expect("mutex").bound = revision;
    }

    fn poll_scoped(&mut self, stop: &AtomicBool) -> Option<Result<ScopedBackendSnapshot, String>> {
        let rx = self.poll_rx.as_ref()?;
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        let (lock, _) = &*self.state;
        let bound = lock.lock().expect("mutex").bound;
        // Wake-driven: returns immediately on send; the bound only caps stop
        // latency. A correct backend stamps the revision it read under.
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => Some(result.map(|mut snapshot| {
                if let Some(revision) = bound {
                    snapshot.revision = revision;
                }
                snapshot
            })),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.poll_rx = None;
                None
            }
        }
    }
}

struct Fixture {
    handle: RuntimeHandle,
    state: Arc<(Mutex<FakeState>, Condvar)>,
    /// Published native topology revision every manual observation fences
    /// against (one display, Spaces 11+12).
    revision: u64,
}

fn start(config: RuntimeConfig) -> Fixture {
    let state = Arc::new((Mutex::new(FakeState::default()), Condvar::new()));
    let handle = Runtime::start(
        RuntimeConfig {
            topology_mode: TopologyMode::Native,
            ..config
        },
        Arc::new(FakeFactory {
            state: Arc::clone(&state),
        }),
        Arc::new(NativeInputStub),
    )
    .expect("runtime starts offline");
    // Manual observations carry no Space membership; the native topology
    // supplies the single 1920x1080 scope the class cases tile into (the
    // same display their synthetic frames already name).
    let revision = handle.update_topology_with(single_display_snapshot(), &user_space);
    Fixture {
        handle,
        state,
        revision,
    }
}

fn single_display_snapshot() -> topology::TopologySnapshot {
    topology::build_snapshot(
        &[ns(1, "uuid-1", 0.0, 0.0, 1920.0, 1080.0)],
        &[row(Some("uuid-1"), &[11, 12], Some(11))],
        Some(true),
    )
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

fn shutdown_clean(fixture: &Fixture) {
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(
        report.outcome,
        ShutdownOutcome::Clean,
        "expected clean shutdown, got {report:?}"
    );
}

// -- Classifier-driven observation helpers ------------------------------------

fn frame(wid: u32, x: i32, y: i32, w: i32, h: i32) -> WindowFrame {
    WindowFrame { wid, x, y, w, h }
}

fn tile(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ClassifiedWindow {
    classify_window(
        frame(wid, x, y, w, h),
        &AxFacts::standard(),
        &CgFacts::normal(),
        &AppFacts::regular(),
    )
}

fn fixed_tile(
    wid: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    hints: WindowSizeHints,
) -> ClassifiedWindow {
    classify_window(
        frame(wid, x, y, w, h),
        &AxFacts {
            hints,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    )
}

fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(480),
    }
}

fn partial_zero_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(0),
        max_w: Some(640),
        max_h: Some(0),
    }
}

fn submit(fixture: &Fixture, pid: u32, generation: u64, seq: u64, items: &[ClassifiedWindow]) {
    submit_focused(fixture, pid, generation, seq, items, None);
}

/// Focus-carrying variant. A sole-window restore must name the restored
/// tile as observed focus (D23 no-focus-write rule): after a sole minimize
/// the session focus is none, and a focus-none restore observation is a
/// focus-mismatch refusal instead of a rejoin.
fn submit_focused(
    fixture: &Fixture,
    pid: u32,
    generation: u64,
    seq: u64,
    items: &[ClassifiedWindow],
    focused_wid: Option<u32>,
) {
    let observed: Vec<ObservedWindow> = items
        .iter()
        .filter_map(ClassifiedWindow::to_observed)
        .collect();
    // Native topology: manual observations are scoped snapshots homed by
    // exact Space membership (11), with neutral truthful metadata and the
    // same agreeing global-focus legs the classifier fixtures attest.
    let (frontmost_pid, ax_focus) = match focused_wid {
        Some(wid) => (Some(pid), Some((pid, Some(wid)))),
        None => (None, None),
    };
    let mut cg_owner_wids: Vec<u32> = observed.iter().map(|w| w.wid).collect();
    cg_owner_wids.sort_unstable();
    cg_owner_wids.dedup();
    let meta: Vec<ScopedWindowMeta> = items
        .iter()
        .filter(|item| item.to_observed().is_some())
        .map(|item| {
            // The classifier's own float origin rides back as the exact
            // native fact that caused it: the scoped route re-derives the
            // exception core-side from facts, never from a synthetic flag.
            let (role, subrole, modal) = match item.disposition {
                Disposition::Float(FloatReason::Sheet) => (
                    Some(ROLE_SHEET.to_owned()),
                    Some(SUBROLE_FLOATING.to_owned()),
                    Some(false),
                ),
                Disposition::Float(FloatReason::DialogSubrole) => (
                    Some(tiler_macos::classifier::ROLE_WINDOW.to_owned()),
                    Some(SUBROLE_DIALOG.to_owned()),
                    Some(false),
                ),
                Disposition::Float(FloatReason::Modal) => (
                    Some(tiler_macos::classifier::ROLE_WINDOW.to_owned()),
                    Some(tiler_macos::classifier::SUBROLE_STANDARD.to_owned()),
                    Some(true),
                ),
                _ => (
                    Some(tiler_macos::classifier::ROLE_WINDOW.to_owned()),
                    Some(tiler_macos::classifier::SUBROLE_STANDARD.to_owned()),
                    Some(false),
                ),
            };
            ScopedWindowMeta {
                wid: item.wid,
                spaces: vec![11],
                fullscreen: item.fullscreen,
                minimized: false,
                hidden: false,
                role,
                subrole,
                modal,
                resizable: Some(true),
                layer: Some(0),
                app_id: Some("com.example.app".to_owned()),
                activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
            }
        })
        .collect();
    assert_eq!(
        fixture.handle.submit_scoped_observation(ScopedObservation {
            pid,
            generation,
            seq,
            windows: observed,
            focused_wid,
            revision: fixture.revision,
            frontmost_pid,
            ax_focus,
            cg_owned: focused_wid.is_some(),
            meta,
            cg_complete: true,
            cg_owner_wids,
        }),
        SubmitOutcome::Accepted
    );
    assert!(
        fixture.handle.flush(FLUSH_TIMEOUT),
        "engine flush timed out"
    );
}

/// Install the production autonomous-observation route for one pid: the
/// owning worker polls this inbox and publishes every snapshot through the
/// same scoped ingestion the live backend uses, classifier exclusions
/// included. Must be called before launch (the worker takes it at creation).
fn backend_observations(
    fixture: &Fixture,
    pid: u32,
) -> mpsc::Sender<Result<ScopedBackendSnapshot, String>> {
    let (tx, rx) = mpsc::channel::<Result<ScopedBackendSnapshot, String>>();
    fixture
        .state
        .0
        .lock()
        .expect("mutex")
        .inboxes
        .insert(pid, rx);
    tx
}

/// One production-shaped scoped snapshot: `live` windows are observed and
/// `minimized` rides the classifier's omission with its truthful
/// `ExcludeReason::Minimized` fact plus a retained meta row (the adapter
/// mints the D23 minimized record from exactly that pair). Every window is
/// CG-owned, minimized ones included.
fn scoped_snapshot(
    live: &[ClassifiedWindow],
    minimized: Option<&ClassifiedWindow>,
) -> ScopedBackendSnapshot {
    let windows: Vec<ObservedWindow> = live
        .iter()
        .filter_map(ClassifiedWindow::to_observed)
        .collect();
    let excluded_reasons: Vec<ExcludedOmission> = minimized
        .map(|item| ExcludedOmission {
            wid: item.wid,
            reason: ExcludeReason::Minimized,
        })
        .into_iter()
        .collect();
    let mut meta: Vec<ScopedWindowMeta> = live
        .iter()
        .map(|item| meta_row(item, false))
        .chain(minimized.map(|item| meta_row(item, true)))
        .collect();
    meta.sort_by_key(|row| row.wid);
    let mut cg_owner_wids: Vec<u32> = live
        .iter()
        .map(|item| item.wid)
        .chain(minimized.map(|item| item.wid))
        .collect();
    cg_owner_wids.sort_unstable();
    cg_owner_wids.dedup();
    ScopedBackendSnapshot {
        windows,
        focused_wid: None,
        focus_cg_owned: false,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        excluded: excluded_reasons.len(),
        excluded_reasons,
        meta,
        cg_complete: true,
        cg_owner_wids,
        cg_alive: Vec::new(),
        revision: 0,
    }
}

fn meta_row(item: &ClassifiedWindow, minimized: bool) -> ScopedWindowMeta {
    ScopedWindowMeta {
        wid: item.wid,
        spaces: vec![11],
        fullscreen: item.fullscreen,
        minimized,
        hidden: false,
        role: Some(ROLE_WINDOW.to_owned()),
        subrole: Some(SUBROLE_STANDARD.to_owned()),
        modal: Some(false),
        resizable: Some(true),
        layer: Some(0),
        app_id: Some("com.example.app".to_owned()),
        activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
    }
}

fn desired_names(fixture: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = fixture
        .handle
        .last_desired_geometry()
        .iter()
        .map(|f| f.window.clone())
        .collect();
    names.sort_unstable();
    names
}

fn desired_rect(fixture: &Fixture, window: &str) -> Option<(i32, i32, i32, i32)> {
    fixture
        .handle
        .last_desired_geometry()
        .iter()
        .find(|f| f.window == window)
        .map(|f| (f.x, f.y, f.w, f.h))
}

fn wait_applied(fixture: &Fixture, window: &str) -> Applied {
    let (lock, wake) = &*fixture.state;
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    let mut guard = lock.lock().expect("mutex");
    loop {
        if let Some(record) = guard.applied.iter().find(|r| r.window == window).cloned() {
            return record;
        }
        let now = std::time::Instant::now();
        assert!(now < deadline, "no applied frame for {window}");
        guard = wake
            .wait_timeout_while(guard, deadline - now, |state| {
                !state.applied.iter().any(|r| r.window == window)
            })
            .expect("mutex")
            .0;
    }
}

fn drain_applied(fixture: &Fixture) {
    fixture.state.0.lock().expect("mutex").applied.clear();
}

/// Causal absence assertion: fence first (the worker has drained every
/// relevant batch through its slot watermark and the engine has processed
/// every tail), then assert immediate absence. Nothing outstanding can
/// arrive late, so this is one check, never a quiet-time poll. The app must
/// be live: a missing generation fails loudly instead of silently skipping
/// the fence while a producer could still be alive.
fn assert_no_applied(fixture: &Fixture, window: &str) {
    let pid: u32 = window
        .split('_')
        .next()
        .and_then(|head| head.parse().ok())
        .expect("window identity names its pid");
    let generation = fixture
        .handle
        .app_generation(pid)
        .expect("absence fence needs a live app generation");
    assert!(
        fixture
            .handle
            .wait_app_settled_for_test(pid, generation, WAIT_TIMEOUT),
        "worker did not settle for {window}"
    );
    let guard = fixture.state.0.lock().expect("mutex");
    assert!(
        !guard.applied.iter().any(|r| r.window == window),
        "unexpected applied frame for {window}"
    );
}

// -- Tests --------------------------------------------------------------------

#[test]
fn normal_tiles_and_sheet_floats_without_writes_or_reflow() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(11, 0, 0, 960, 1080), tile(12, 960, 0, 960, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11", "1001_12"]);
    wait_applied(&fixture, "1001_11");
    wait_applied(&fixture, "1001_12");
    let before_11 = desired_rect(&fixture, "1001_11").expect("tile planned");
    let before_12 = desired_rect(&fixture, "1001_12").expect("tile planned");
    let emitted_before = fixture.handle.diagnostics().frames_emitted;
    drain_applied(&fixture);

    // A sheet arrives: it must float as a normalized exception member — no
    // tile allocation, no geometry write, siblings untouched.
    let sheet = classify_window(
        frame(13, 480, 270, 960, 540),
        &AxFacts {
            role: Some(ROLE_SHEET.to_owned()),
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(matches!(sheet.disposition, Disposition::Float(_)));
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[
            tile(11, 0, 0, 960, 1080),
            tile(12, 960, 0, 960, 1080),
            sheet,
        ],
    );
    // Siblings keep their exact allocation (no reflow for the float).
    assert_eq!(desired_names(&fixture), vec!["1001_11", "1001_12"]);
    assert_eq!(desired_rect(&fixture, "1001_11"), Some(before_11));
    assert_eq!(desired_rect(&fixture, "1001_12"), Some(before_12));
    // Only the two tiles re-emit; the sheet takes no write.
    assert_eq!(
        fixture.handle.diagnostics().frames_emitted,
        emitted_before + 2,
        "only tiled members emit frames"
    );
    assert_no_applied(&fixture, "1001_13");
    shutdown_clean(&fixture);
}

#[test]
fn dialog_subrole_floats_without_sibling_allocation() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let dialog = classify_window(
        frame(13, 640, 390, 640, 300),
        &AxFacts {
            subrole: Some(SUBROLE_DIALOG.to_owned()),
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(matches!(dialog.disposition, Disposition::Float(_)));
    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(11, 0, 0, 1920, 1080), dialog],
    );
    // The lone tile keeps the whole domain; the dialog is membership only.
    assert_eq!(desired_names(&fixture), vec!["1001_11"]);
    wait_applied(&fixture, "1001_11");
    assert_no_applied(&fixture, "1001_13");
    shutdown_clean(&fixture);
}

#[test]
fn fixed_hints_float_automatic_under_both_axes() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    // Seed the domain with an ordinary tile first: arrivals then classify
    // through convergence (the realistic launch-into-live-session flow).
    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(12, 0, 0, 1920, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_12"]);
    wait_applied(&fixture, "1001_12");
    drain_applied(&fixture);

    // Fixed-size arrival floats automatic (membership only, D8): the
    // ordinary sibling tiles alone and the fixed window takes no write.
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[
            fixed_tile(11, 0, 0, 640, 480, fixed_hints()),
            tile(12, 0, 0, 960, 1080),
        ],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_12"]);
    wait_applied(&fixture, "1001_12");
    assert_no_applied(&fixture, "1001_11");
    drain_applied(&fixture);

    // G-05 partial-zero is NOT fixed under both-axes: it tiles normally.
    submit(
        &fixture,
        APP_A,
        generation,
        3,
        &[
            fixed_tile(11, 0, 0, 640, 480, fixed_hints()),
            tile(12, 0, 0, 960, 1080),
            fixed_tile(13, 0, 0, 640, 100, partial_zero_hints()),
        ],
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_12", "1001_13"],
        "partial-zero tiles under both-axes-fixed"
    );
    wait_applied(&fixture, "1001_13");
    shutdown_clean(&fixture);
}

#[test]
fn partial_zero_floats_automatic_under_either_axis() {
    // The native v4 admission predicate is core-owned settings (the adapter
    // field only feeds the synthetic skeleton engine), so the test acquires
    // the either-axis policy the way production does: raw settings at
    // startup, validated by the core.
    let fixture = start(RuntimeConfig {
        core_settings: vec![(
            "predicate".to_owned(),
            tiler_core::connection::RawSettingValue::String("either-axis-fixed".to_owned()),
        )],
        ..RuntimeConfig::default()
    });
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(12, 0, 0, 1920, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_12"]);
    wait_applied(&fixture, "1001_12");
    drain_applied(&fixture);

    // The genuinely fixed nonzero axis pins under either-axis-fixed.
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[
            fixed_tile(13, 0, 0, 640, 100, partial_zero_hints()),
            tile(12, 0, 0, 960, 1080),
        ],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_12"]);
    wait_applied(&fixture, "1001_12");
    assert_no_applied(&fixture, "1001_13");
    shutdown_clean(&fixture);
}

#[test]
fn unsettable_unhinted_ghostty_window_tiles_normally() {
    // TBD lane pin at the engine boundary: no resizability inference, and
    // Ghostty (ordinary regular app) carries no product exclusion.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let unsettable = classify_window(
        frame(11, 0, 0, 960, 1080),
        &AxFacts {
            size_settable: false,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert_eq!(unsettable.disposition, Disposition::Tile);
    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[unsettable, tile(12, 960, 0, 960, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11", "1001_12"]);
    wait_applied(&fixture, "1001_11");
    wait_applied(&fixture, "1001_12");
    shutdown_clean(&fixture);
}

/// Live-arrival variant of the cold hold above: a born fullscreen window
/// arriving on an already-seeded domain holds through convergence (not the
/// fresh path), with the same slotless exit semantics.
#[test]
fn live_born_fullscreen_arrival_holds_on_seeded_domain() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let born = |wid| {
        classify_window(
            frame(wid, 0, 0, 1920, 1080),
            &AxFacts {
                fullscreen: true,
                ..AxFacts::standard()
            },
            &CgFacts::normal(),
            &AppFacts::regular(),
        )
    };
    // Seed the domain first: born arrivals hold through convergence on the
    // live session (a game launching into a live desktop, not into a void).
    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(10, 0, 0, 1920, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10"]);
    wait_applied(&fixture, "1001_10");
    drain_applied(&fixture);

    // Born fullscreen holds slotless: no tile, no write, seed tile intact.
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[tile(10, 0, 0, 1920, 1080), born(21)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10"]);
    wait_applied(&fixture, "1001_10");
    assert_no_applied(&fixture, "1001_21");
    drain_applied(&fixture);

    // First exit with current fixed hints floats automatic; a second born
    // arrival in the same observation still holds.
    let exited_fixed = fixed_tile(21, 0, 0, 640, 480, fixed_hints());
    submit(
        &fixture,
        APP_A,
        generation,
        3,
        &[tile(10, 0, 0, 1920, 1080), exited_fixed.clone(), born(22)],
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_10"],
        "fixed first exit floats, second born holds"
    );
    wait_applied(&fixture, "1001_10");
    assert_no_applied(&fixture, "1001_21");
    assert_no_applied(&fixture, "1001_22");
    drain_applied(&fixture);

    // A hintless first exit tiles outright.
    submit(
        &fixture,
        APP_A,
        generation,
        4,
        &[
            tile(10, 0, 0, 1920, 1080),
            exited_fixed,
            tile(22, 0, 0, 1920, 1080),
        ],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10", "1001_22"]);
    wait_applied(&fixture, "1001_22");
    assert_no_applied(&fixture, "1001_21");
    shutdown_clean(&fixture);
}

#[test]
fn retained_fullscreen_keeps_slot_without_writes() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(11, 0, 0, 1920, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11"]);
    let first = wait_applied(&fixture, "1001_11");
    drain_applied(&fixture);

    // The retained tile goes native fullscreen: membership (slot) is kept
    // while the emission filter withholds every geometry write.
    let overlay = classify_window(
        frame(11, 0, 0, 1920, 1080),
        &AxFacts {
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert_eq!(overlay.disposition, Disposition::Fullscreen);
    submit(&fixture, APP_A, generation, 2, &[overlay]);
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics.frames_overlay_skipped >= 1,
        "overlay write withheld: {diagnostics:?}"
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_11"],
        "retained slot survives the overlay in plan truth"
    );
    assert_no_applied(&fixture, "1001_11");

    // Exit returns to the kept slot with the same geometry.
    submit(
        &fixture,
        APP_A,
        generation,
        3,
        &[tile(11, 0, 0, 1920, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11"]);
    let second = wait_applied(&fixture, "1001_11");
    assert_eq!(second.rect, first.rect, "exit restores the kept slot");
    assert!(second.cmd_id > first.cmd_id);
    shutdown_clean(&fixture);
}

#[test]
fn minimized_window_leaves_layout_and_rejoins_on_restore() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(11, 0, 0, 960, 1080), tile(12, 960, 0, 960, 1080)],
    );
    wait_applied(&fixture, "1001_11");
    wait_applied(&fixture, "1001_12");
    let two_up = desired_rect(&fixture, "1001_12").expect("tile planned");
    drain_applied(&fixture);

    // Minimized windows are omitted from the submission (classifier
    // exclusion): layout releases the allocation and stores the restore
    // slot instead of reporting a close.
    let minimized = classify_window(
        frame(11, 0, 0, 960, 1080),
        &AxFacts {
            minimized: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(matches!(minimized.disposition, Disposition::Excluded(_)));
    assert!(minimized.to_observed().is_none());
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[tile(12, 960, 0, 960, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_12"]);
    assert_ne!(
        desired_rect(&fixture, "1001_12"),
        Some(two_up),
        "survivor reflows into the released allocation"
    );

    // Restore rejoins (D23 slot) instead of vanishing or duplicating.
    submit(
        &fixture,
        APP_A,
        generation,
        3,
        &[tile(11, 0, 0, 960, 1080), tile(12, 960, 0, 960, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11", "1001_12"]);
    wait_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn hidden_app_windows_leave_layout_and_recover() {
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(11, 0, 0, 1920, 1080)],
    );
    wait_applied(&fixture, "1001_11");
    drain_applied(&fixture);

    // Hidden-app windows omit like minimized ones (Silica visibleWindows
    // parity): absence is authoritative loss, never destruction.
    let hidden = classify_window(
        frame(11, 0, 0, 1920, 1080),
        &AxFacts::standard(),
        &CgFacts::normal(),
        &AppFacts {
            hidden: true,
            ..AppFacts::regular()
        },
    );
    assert!(matches!(hidden.disposition, Disposition::Excluded(_)));
    submit(&fixture, APP_A, generation, 2, &[]);
    assert!(desired_names(&fixture).is_empty());
    assert_no_applied(&fixture, "1001_11");

    submit_focused(
        &fixture,
        APP_A,
        generation,
        3,
        &[tile(11, 0, 0, 1920, 1080)],
        Some(11),
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11"]);
    wait_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn retired_exception_only_domain_reseeds_cleanly() {
    // A sole transient leaves no restore slot when it goes away: the
    // domain retires, the revision cursor resets with it, and a later
    // ordinary tile reseeds instead of diverging on a stale-high revision.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let sheet = classify_window(
        frame(13, 480, 270, 960, 540),
        &AxFacts {
            role: Some(ROLE_SHEET.to_owned()),
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(&fixture, APP_A, generation, 1, &[sheet]);
    assert!(desired_names(&fixture).is_empty());
    submit(&fixture, APP_A, generation, 2, &[]);
    assert!(desired_names(&fixture).is_empty());
    submit(
        &fixture,
        APP_A,
        generation,
        3,
        &[tile(11, 0, 0, 1920, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_11"]);
    wait_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

fn has_reply(fixture: &Fixture, token: &str) -> bool {
    fixture
        .handle
        .diagnostics()
        .core_replies
        .get(token)
        .is_some_and(|count| *count > 0)
}

#[test]
fn cold_born_fullscreen_only_holds_slotless() {
    // Cold first observation is born fullscreen with no seeded domain: the
    // core must hold it slotless (projection, no tile, no write) — never
    // refuse the observation.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let born = classify_window(
        frame(21, 0, 0, 1920, 1080),
        &AxFacts {
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(&fixture, APP_A, generation, 1, &[born]);
    assert!(
        desired_names(&fixture).is_empty(),
        "cold born holds slotless"
    );
    assert_eq!(fixture.handle.diagnostics().frames_emitted, 0);
    assert!(
        has_reply(&fixture, "reconcile-accepted"),
        "hold converges, never refuses: {:?}",
        fixture.handle.diagnostics().core_replies
    );

    // First exit with no hints tiles outright. The held session has no
    // focus, so the exit names observed focus (same D23 fence as sole
    // restore: focus-none exit observations refuse instead of rejoining).
    submit_focused(
        &fixture,
        APP_A,
        generation,
        2,
        &[tile(21, 0, 0, 1920, 1080)],
        Some(21),
    );
    assert_eq!(desired_names(&fixture), vec!["1001_21"]);
    wait_applied(&fixture, "1001_21");
    drain_applied(&fixture);

    // Re-entered fullscreen on the now-known member stays truthful: the
    // reserved allocation survives in plan truth with no overlay write.
    let overlay = classify_window(
        frame(21, 0, 0, 1920, 1080),
        &AxFacts {
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(&fixture, APP_A, generation, 3, &[overlay]);
    assert_eq!(desired_names(&fixture), vec!["1001_21"]);
    assert!(
        fixture.handle.diagnostics().frames_overlay_skipped >= 1,
        "member overlay write withheld"
    );
    assert_no_applied(&fixture, "1001_21");
    shutdown_clean(&fixture);
}

#[test]
fn cold_fixed_only_floats_automatic() {
    // Cold first observation carries fixed hints only: admission floats it
    // automatic (membership only, D8) instead of refusing.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[fixed_tile(11, 0, 0, 640, 480, fixed_hints())],
    );
    assert_eq!(
        desired_names(&fixture),
        Vec::<String>::new(),
        "cold fixed floats automatic"
    );
    assert!(
        has_reply(&fixture, "reconcile-accepted"),
        "admission converges, never refuses: {:?}",
        fixture.handle.diagnostics().core_replies
    );
    assert_no_applied(&fixture, "1001_11");
    drain_applied(&fixture);

    // D2: hint loss on the retained automatic never reclassifies — it stays
    // floating with its origin instead of tiling.
    submit(&fixture, APP_A, generation, 2, &[tile(11, 0, 0, 640, 480)]);
    assert_eq!(
        desired_names(&fixture),
        Vec::<String>::new(),
        "hint loss keeps the automatic float (D2)"
    );
    assert_no_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn cold_mixed_fixed_and_tile_first_observation() {
    // Cold first observation mixes a fixed window with a plain tile: the
    // fixed member floats automatic while the tile seeds normally.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[
            fixed_tile(11, 0, 0, 640, 480, fixed_hints()),
            tile(12, 0, 0, 960, 1080),
        ],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_12"]);
    assert!(
        has_reply(&fixture, "reconcile-accepted"),
        "mixed seed converges: {:?}",
        fixture.handle.diagnostics().core_replies
    );
    wait_applied(&fixture, "1001_12");
    assert_no_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn two_tile_retained_fullscreen_keeps_both_allocations() {
    // Two retained tiles, one goes native fullscreen: BOTH reserved
    // allocations stay in plan truth (slot retained, exact rects) while the
    // overlay member takes no geometry write.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(10, 0, 0, 960, 1080), tile(11, 960, 0, 960, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10", "1001_11"]);
    wait_applied(&fixture, "1001_10");
    wait_applied(&fixture, "1001_11");
    let reserved_10 = desired_rect(&fixture, "1001_10").expect("tile planned");
    let reserved_11 = desired_rect(&fixture, "1001_11").expect("tile planned");
    drain_applied(&fixture);

    let overlay = classify_window(
        frame(11, 960, 0, 960, 1080),
        &AxFacts {
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[tile(10, 0, 0, 960, 1080), overlay],
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_10", "1001_11"],
        "both reserved allocations survive the overlay in plan truth"
    );
    assert_eq!(desired_rect(&fixture, "1001_10"), Some(reserved_10));
    assert_eq!(desired_rect(&fixture, "1001_11"), Some(reserved_11));
    assert!(
        fixture.handle.diagnostics().frames_overlay_skipped >= 1,
        "overlay write withheld"
    );
    wait_applied(&fixture, "1001_10");
    assert_no_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn fullscreen_exit_reclassifies_fresh_hints_not_birth_hints() {
    // Born WITH fixed hints, exiting with NONE: fresh hints win, so the
    // window tiles. Born with none, exiting fixed: floats automatic.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let born_fixed = classify_window(
        frame(21, 0, 0, 1920, 1080),
        &AxFacts {
            fullscreen: true,
            hints: fixed_hints(),
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    let born_plain = classify_window(
        frame(22, 0, 0, 1920, 1080),
        &AxFacts {
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(&fixture, APP_A, generation, 1, &[born_fixed, born_plain]);
    assert!(
        desired_names(&fixture).is_empty(),
        "both born arrivals hold (birth hints decide nothing)"
    );
    assert!(
        has_reply(&fixture, "reconcile-accepted"),
        "cold double hold converges: {:?}",
        fixture.handle.diagnostics().core_replies
    );

    submit_focused(
        &fixture,
        APP_A,
        generation,
        2,
        &[
            tile(21, 0, 0, 1920, 1080),
            fixed_tile(22, 0, 0, 640, 480, fixed_hints()),
        ],
        Some(21),
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_21"],
        "hintless exit tiles despite fixed birth hints; fixed exit floats"
    );
    wait_applied(&fixture, "1001_21");
    assert_no_applied(&fixture, "1001_22");
    shutdown_clean(&fixture);
}

#[test]
fn dialog_with_fullscreen_holds_truthful_without_writes() {
    // A dialog reporting fullscreen carries BOTH facts: exception member
    // (no tile, no write) with the overlay flag truthful.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let dialog_fs = classify_window(
        frame(13, 640, 390, 640, 300),
        &AxFacts {
            subrole: Some(SUBROLE_DIALOG.to_owned()),
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(matches!(
        dialog_fs.disposition,
        Disposition::Float(FloatReason::DialogSubrole)
    ));
    let observed = dialog_fs.to_observed().expect("dialog submits");
    assert!(
        observed.fullscreen,
        "fullscreen fact survives on dialog exceptions"
    );
    submit(&fixture, APP_A, generation, 1, &[dialog_fs]);
    assert!(
        desired_names(&fixture).is_empty(),
        "fullscreen dialog takes no allocation"
    );
    assert!(
        has_reply(&fixture, "reconcile-accepted"),
        "cold dialog hold converges: {:?}",
        fixture.handle.diagnostics().core_replies
    );
    assert_no_applied(&fixture, "1001_13");
    drain_applied(&fixture);

    // Exit fullscreen staying a dialog: plain float exception, still no
    // allocation and no write.
    let dialog = classify_window(
        frame(13, 640, 390, 640, 300),
        &AxFacts {
            subrole: Some(SUBROLE_DIALOG.to_owned()),
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(&fixture, APP_A, generation, 2, &[dialog]);
    assert!(desired_names(&fixture).is_empty());
    assert_no_applied(&fixture, "1001_13");
    drain_applied(&fixture);

    // Retained dialog going fullscreen: truthful overlay exception, still
    // no allocation and no write.
    let dialog_fs_again = classify_window(
        frame(13, 640, 390, 640, 300),
        &AxFacts {
            subrole: Some(SUBROLE_DIALOG.to_owned()),
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    let observed_again = dialog_fs_again.to_observed().expect("dialog submits");
    assert!(observed_again.floating && observed_again.fullscreen);
    submit(&fixture, APP_A, generation, 3, &[dialog_fs_again]);
    assert!(desired_names(&fixture).is_empty());
    assert_no_applied(&fixture, "1001_13");
    shutdown_clean(&fixture);
}

#[test]
fn nonzero_layer_floating_panel_takes_no_allocation() {
    // A floating panel at a nonzero native level keeps its float
    // exception: the tile keeps its full allocation and the panel takes
    // no geometry write.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    let panel = classify_window(
        frame(13, 1400, 100, 480, 300),
        &AxFacts {
            subrole: Some(SUBROLE_FLOATING.to_owned()),
            ..AxFacts::standard()
        },
        &CgFacts {
            layer: 3,
            onscreen: true,
        },
        &AppFacts::regular(),
    );
    assert!(matches!(
        panel.disposition,
        Disposition::Float(FloatReason::DialogSubrole)
    ));
    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(10, 0, 0, 1920, 1080), panel],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10"]);
    // The v4 native domain is the gap-inset world rect (outer gap 8 on a
    // 1920x1080 display), so a lone tile keeps that whole domain and the
    // floating panel still steals nothing from it.
    assert_eq!(
        desired_rect(&fixture, "1001_10"),
        Some((8, 8, 1904, 1064)),
        "lone tile keeps the whole domain, panel steals nothing"
    );
    wait_applied(&fixture, "1001_10");
    assert_no_applied(&fixture, "1001_13");
    shutdown_clean(&fixture);
}

#[test]
fn minimized_fullscreen_return_fresh_admits_on_exit() {
    // Current parity (tentative A): a minimized middle window that returns
    // straight into fullscreen reports truthfully on the wire, so the
    // existing core drops the stored D23 slot (present-again exception
    // history) and holds it slotless without disturbing survivors. Its
    // plain exit then fresh-admits instead of restoring the saved middle.
    // Asymmetric 3-window discriminator: a plain D23 restore would return
    // the middle window to its exact middle allocation (see the untouched
    // plain oracle below); here the returnee lands elsewhere.
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");

    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[
            tile(10, 0, 0, 640, 1080),
            tile(11, 640, 0, 640, 1080),
            tile(12, 1280, 0, 640, 1080),
        ],
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_10", "1001_11", "1001_12"]
    );
    wait_applied(&fixture, "1001_10");
    wait_applied(&fixture, "1001_11");
    wait_applied(&fixture, "1001_12");
    let birth_11 = desired_rect(&fixture, "1001_11").expect("tile planned");
    drain_applied(&fixture);

    // Minimized middle: omitted, slot stored, survivors reflow.
    submit(
        &fixture,
        APP_A,
        generation,
        2,
        &[tile(10, 0, 0, 640, 1080), tile(12, 1280, 0, 640, 1080)],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10", "1001_12"]);
    let pair_10 = desired_rect(&fixture, "1001_10").expect("tile planned");
    let pair_12 = desired_rect(&fixture, "1001_12").expect("tile planned");
    drain_applied(&fixture);

    // Truthful fullscreen return: no tile, no write, survivors untouched.
    let returning_fs = classify_window(
        frame(11, 640, 0, 640, 1080),
        &AxFacts {
            fullscreen: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    submit(
        &fixture,
        APP_A,
        generation,
        3,
        &[
            tile(10, 0, 0, 640, 1080),
            returning_fs,
            tile(12, 1280, 0, 640, 1080),
        ],
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10", "1001_12"]);
    assert_eq!(
        desired_rect(&fixture, "1001_10"),
        Some(pair_10),
        "truthful fullscreen return disturbs nothing"
    );
    assert_eq!(
        desired_rect(&fixture, "1001_12"),
        Some(pair_12),
        "truthful fullscreen return disturbs nothing"
    );
    assert_no_applied(&fixture, "1001_11");
    drain_applied(&fixture);

    // Plain exit fresh-admits: the returnee does NOT land back in its
    // saved middle allocation, unlike a plain D23 restore.
    submit(
        &fixture,
        APP_A,
        generation,
        4,
        &[
            tile(10, 0, 0, 640, 1080),
            tile(11, 640, 0, 640, 1080),
            tile(12, 1280, 0, 640, 1080),
        ],
    );
    assert_eq!(
        desired_names(&fixture),
        vec!["1001_10", "1001_11", "1001_12"]
    );
    let fresh_11 = desired_rect(&fixture, "1001_11").expect("tile planned");
    assert_ne!(
        fresh_11, birth_11,
        "saved middle slot was dropped: plain exit fresh-admits instead of restoring"
    );
    wait_applied(&fixture, "1001_11");
    shutdown_clean(&fixture);
}

#[test]
fn plain_minimize_restore_is_position_exact() {
    // Contrast pin for the hold-intervened limitation above: a plain
    // minimize/restore roundtrip with no fullscreen interlude restores
    // exact positions, not just the rect set.
    //
    // Driven through the production route: the backend reports the
    // minimized window as the classifier's omission with its truthful
    // `ExcludeReason::Minimized` fact (the manual
    // `submit_scoped_observation` leg always reports `excluded: 0`, so it
    // cannot express a minimize at all). That fact is what mints the
    // adapter's retained minimized record and, with it, the core's D23
    // restore slot.
    let fixture = start(RuntimeConfig::default());
    let inbox = backend_observations(&fixture, APP_A);
    let _generation = fixture.handle.launch_app(APP_A).expect("launch");
    let tile_10 = tile(10, 0, 0, 960, 1080);
    let tile_11 = tile(11, 960, 0, 960, 1080);

    inbox
        .send(Ok(scoped_snapshot(
            &[tile_10.clone(), tile_11.clone()],
            None,
        )))
        .expect("birth snapshot");
    wait_applied(&fixture, "1001_10");
    wait_applied(&fixture, "1001_11");
    let birth_10 = desired_rect(&fixture, "1001_10").expect("tile planned");
    let birth_11 = desired_rect(&fixture, "1001_11").expect("tile planned");
    drain_applied(&fixture);

    // Minimize: the window leaves membership and is reported as a
    // classified omission (never a silent absence), so the survivor reflows
    // into the released allocation and no frame is written to the hidden
    // window.
    let minimized = classify_window(
        frame(11, 960, 0, 960, 1080),
        &AxFacts {
            minimized: true,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(
        minimized.to_observed().is_none(),
        "a minimized window publishes as an omission, not an observed tile"
    );
    inbox
        .send(Ok(scoped_snapshot(&[tile_10.clone()], Some(&minimized))))
        .expect("minimize snapshot");
    let reflowed = wait_applied(&fixture, "1001_10");
    assert_ne!(
        reflowed.rect, birth_10,
        "survivor reflows into the released allocation"
    );
    assert_eq!(desired_names(&fixture), vec!["1001_10"]);
    assert_no_applied(&fixture, "1001_11");
    drain_applied(&fixture);

    // Restore rejoins the stored D23 slot exactly: same rects as birth,
    // not merely the same set of windows.
    inbox
        .send(Ok(scoped_snapshot(
            &[tile_10.clone(), tile_11.clone()],
            None,
        )))
        .expect("restore snapshot");
    wait_applied(&fixture, "1001_11");
    assert_eq!(desired_names(&fixture), vec!["1001_10", "1001_11"]);
    assert_eq!(desired_rect(&fixture, "1001_10"), Some(birth_10));
    assert_eq!(desired_rect(&fixture, "1001_11"), Some(birth_11));
    shutdown_clean(&fixture);
}

#[test]
fn own_bundle_accessory_and_prohibited_never_submit() {
    // Policy exclusions are classifier-side: none of these shapes may ever
    // reach the engine as an observation. The engine half is covered by the
    // empty-observation path (absence, never close).
    let truthy = |app: &AppFacts| {
        classify_window(
            frame(11, 0, 0, 800, 600),
            &AxFacts::standard(),
            &CgFacts::normal(),
            app,
        )
    };
    let own = AppFacts {
        is_own_bundle: true,
        ..AppFacts::regular()
    };
    assert!(truthy(&own).to_observed().is_none());
    let accessory = AppFacts {
        activation_policy: ActivationPolicy::Accessory,
        ..AppFacts::regular()
    };
    assert!(truthy(&accessory).to_observed().is_none());
    let prohibited = AppFacts {
        activation_policy: ActivationPolicy::Prohibited,
        ..AppFacts::regular()
    };
    assert!(truthy(&prohibited).to_observed().is_none());

    // And the engine treats a policy-emptied app as absent, not destroyed:
    // an empty submission converges (no close report exists on this path).
    let fixture = start(RuntimeConfig::default());
    let generation = fixture.handle.launch_app(APP_A).expect("launch");
    submit(
        &fixture,
        APP_A,
        generation,
        1,
        &[tile(11, 0, 0, 1920, 1080)],
    );
    wait_applied(&fixture, "1001_11");
    submit(&fixture, APP_A, generation, 2, &[]);
    assert!(desired_names(&fixture).is_empty());
    shutdown_clean(&fixture);
}

#[test]
fn unknown_roles_exclude_at_the_boundary() {
    // Unknown subroles (tooltips, pills, switchers) and non-window roles
    // never reach the engine; the standard subrole is the only tile shape.
    let unknown = classify_window(
        frame(11, 0, 0, 240, 32),
        &AxFacts {
            subrole: Some("AXUnknown".to_owned()),
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(unknown.to_observed().is_none());
    let missing = classify_window(
        frame(11, 0, 0, 240, 32),
        &AxFacts {
            subrole: None,
            ..AxFacts::standard()
        },
        &CgFacts::normal(),
        &AppFacts::regular(),
    );
    assert!(missing.to_observed().is_none());
    let layered = classify_window(
        frame(11, 0, 0, 800, 600),
        &AxFacts::standard(),
        &CgFacts {
            layer: 25,
            onscreen: true,
        },
        &AppFacts::regular(),
    );
    assert!(layered.to_observed().is_none());
    // Off-Space aliveness (`onscreen=false`, normal layer) keeps its facts.
    let off_space = classify_window(
        frame(11, 0, 0, 800, 600),
        &AxFacts::standard(),
        &CgFacts {
            layer: 0,
            onscreen: false,
        },
        &AppFacts::regular(),
    );
    assert_eq!(off_space.disposition, Disposition::Tile);
}

