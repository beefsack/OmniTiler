#![forbid(unsafe_code)]

//! Native outer gaps, Tier-1 focus walk, and
//! change-only trace emissions (offline, portable).
//!
//! - Outer gaps: the adapter insets the visible work area by the shared
//!   outer margin (KDE/Windows 8/8 parity) before the core plans, so applied
//!   frames keep the edge margin with inner sibling spacing.
//! - Focus: repeated directional steps walk the row and retain at the exhausted
//!   edge (shared Tier-1 rule: no wrap, no workspace cycling), counted as
//!   `focus-rejected`, with later steps still dispatching after the miss.
//! - Traces: republishing identical focus evidence stays ring-silent; only
//!   genuine changes emit.
//!
//! Fakes drive the runtime through the native surface: revision-fenced
//! topology contexts, membership-carrying scoped snapshots, owner-dispatched
//! focus. No live AX, no taps, no Spaces, no desktop effects.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};
use std::time::Duration;

use tiler_macos::config::ProbeModifier;
use tiler_macos::ffi::{DisplayRect, NsDisplay, SlsDisplayRow};
use tiler_macos::input::cg_flag_bits;
use tiler_macos::input_match::TapKeyKind;
use tiler_macos::runtime::{
    AxCompletion, FocusCommand, FocusEvidence, ObservedWindow, Runtime, RuntimeConfig,
    RuntimeHandle, ScopedBackendSnapshot, ScopedObservation, ScopedWindowMeta, ShutdownOutcome,
    SubmitOutcome, TopologyMode,
};
use tiler_macos::runtime_backend::{
    AxBackend, AxBackendFactory, InputSource, InputSourceFactory, RawKeyEvent,
};
use tiler_macos::topology;

const APP_A: u32 = 1001;
const FLUSH: Duration = Duration::from_secs(5);
const WAIT: Duration = Duration::from_secs(5);
const OUTER: i32 = 8;
const INNER: i32 = 8;

/// One applied frame: pid, cmd id, window id, rect, topology revision.
type AppliedRecord = (u32, u64, String, (i32, i32, i32, i32), u64);

struct FakeInner {
    applied_tx: mpsc::Sender<AppliedRecord>,
    scoped: Mutex<BTreeMap<u32, ScopedBackendSnapshot>>,
    focus_tx: mpsc::Sender<FocusCommand>,
}

struct FakeFactory {
    inner: Arc<FakeInner>,
}

impl AxBackendFactory for FakeFactory {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String> {
        Ok(Box::new(FakeBackend {
            pid,
            inner: Arc::clone(&self.inner),
            bound: None,
        }))
    }
}

struct FakeBackend {
    pid: u32,
    inner: Arc<FakeInner>,
    bound: Option<u64>,
}

impl AxBackend for FakeBackend {
    fn apply_frame(&mut self, cmd: &tiler_macos::runtime::FrameCommand) -> AxCompletion {
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
        Err("legacy snapshot refused (native tests use scoped reads)".to_owned())
    }

    fn bind_effect_hook(&mut self, hook: &tiler_macos::runtime_backend::EffectHook) {
        self.bound = hook.cell.load().map(|ctx| ctx.revision);
    }

    fn snapshot_scoped(&mut self) -> Result<ScopedBackendSnapshot, String> {
        let revision = self
            .bound
            .ok_or_else(|| "no native context bound".to_owned())?;
        let mut snapshot = self
            .inner
            .scoped
            .lock()
            .expect("mutex")
            .get(&self.pid)
            .cloned()
            .ok_or_else(|| "no scoped snapshot".to_owned())?;
        snapshot.revision = revision;
        Ok(snapshot)
    }

    fn poll_scoped(&mut self, _stop: &AtomicBool) -> Option<Result<ScopedBackendSnapshot, String>> {
        None
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
                    if stop.load(std::sync::atomic::Ordering::SeqCst) {
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

fn start() -> Fixture {
    let (applied_tx, applied_rx) = mpsc::channel();
    let (focus_tx, focus_rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel();
    let inner = Arc::new(FakeInner {
        applied_tx,
        scoped: Mutex::new(BTreeMap::new()),
        focus_tx,
    });
    let config = RuntimeConfig {
        topology_mode: TopologyMode::Native,
        ..RuntimeConfig::default()
    };
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

fn one_display() -> topology::TopologySnapshot {
    topology::build_snapshot(
        &[NsDisplay {
            cg_id: 1,
            uuid: Some("uuid-1".to_owned()),
            frame: DisplayRect {
                x: 0.0,
                y: 0.0,
                w: 1440.0,
                h: 900.0,
            },
            visible: DisplayRect {
                x: 0.0,
                y: 0.0,
                w: 1440.0,
                h: 900.0,
            },
            scale: 2.0,
        }],
        &[SlsDisplayRow {
            identifier: Some("uuid-1".to_owned()),
            spaces: vec![11],
            current: Some(11),
        }],
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
        hints: tiler_macos::runtime::WindowSizeHints::none(),
    }
}

fn meta(wid: u32) -> ScopedWindowMeta {
    ScopedWindowMeta {
        wid,
        spaces: vec![11],
        fullscreen: false,
        minimized: false,
        hidden: false,
    }
}

fn scripted(windows: Vec<ObservedWindow>, focused: Option<u32>) -> ScopedBackendSnapshot {
    let meta = windows.iter().map(|w| meta(w.wid)).collect();
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

fn agreeing(pid: u32, wid: u32) -> FocusEvidence {
    FocusEvidence {
        frontmost_pid: Some(pid),
        frontmost_bundle: None,
        ax_focus: Some((pid, Some(wid))),
    }
}

/// Manual arrival observation with agreeing global legs (frontmost, AX and
/// CG all name the reporter), so the engine promotes the reported focus.
fn arrival(
    pid: u32,
    generation: u64,
    seq: u64,
    windows: Vec<ObservedWindow>,
    focused_wid: u32,
    revision: u64,
) -> ScopedObservation {
    let meta = windows.iter().map(|w| meta(w.wid)).collect();
    ScopedObservation {
        pid,
        generation,
        seq,
        windows,
        focused_wid: Some(focused_wid),
        revision,
        frontmost_pid: Some(pid),
        ax_focus: Some((pid, Some(focused_wid))),
        cg_owned: true,
        meta,
    }
}

fn send_focus_right(fixture: &Fixture) {
    // Legacy focus chord (H key) with the default Right step direction.
    let flags = cg_flag_bits(ProbeModifier::Ctrl).0;
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code: 0x04,
        })
        .expect("send focus chord");
}

fn collect_frames(fixture: &Fixture, window: &str, want: usize) -> Vec<AppliedRecord> {
    let deadline = std::time::Instant::now() + WAIT;
    let mut out = Vec::new();
    while out.len() < want && std::time::Instant::now() < deadline {
        match fixture.applied_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(record) if record.2 == window => out.push(record),
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    out
}

fn shutdown_clean(fixture: &Fixture) {
    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn native_single_window_keeps_outer_margin() {
    // One window fills the inset work area exactly: 8px of outer margin on
    // every side of the 1440x900 visible frame.
    let fixture = start();
    fixture.handle.update_topology_with(one_display(), &|_| {
        Some(tiler_macos::ffi::SlsSpaceType::User)
    });
    install(
        &fixture,
        APP_A,
        scripted(vec![win_at(1, 0, 0, 100, 100)], Some(1)),
    );
    fixture.handle.update_focus_evidence(agreeing(APP_A, 1));
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    let frames = collect_frames(&fixture, "1001_1", 1);
    assert!(!frames.is_empty(), "no applied frame arrived");
    let rect = frames.into_iter().map(|r| r.3).next().unwrap();
    assert_eq!(
        rect,
        (OUTER, OUTER, 1440 - 2 * OUTER, 900 - 2 * OUTER),
        "single frame must carry the outer margin, got {rect:?}"
    );
    shutdown_clean(&fixture);
}

#[test]
fn native_row_keeps_outer_margin_and_inner_spacing() {
    // Three side-by-side windows: outer margin on the area edges, 8px inner
    // spacing between siblings, full-height columns.
    let fixture = start();
    fixture.handle.update_topology_with(one_display(), &|_| {
        Some(tiler_macos::ffi::SlsSpaceType::User)
    });
    install(
        &fixture,
        APP_A,
        scripted(
            vec![
                win_at(1, 8, 8, 466, 884),
                win_at(2, 482, 8, 466, 884),
                win_at(3, 956, 8, 468, 884),
            ],
            Some(1),
        ),
    );
    fixture.handle.update_focus_evidence(agreeing(APP_A, 1));
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    let mut rects = BTreeMap::new();
    let deadline = std::time::Instant::now() + WAIT;
    while rects.len() < 3 && std::time::Instant::now() < deadline {
        match fixture.applied_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(record) => {
                rects.insert(record.2.clone(), record.3);
            }
            Err(_) => break,
        }
    }
    let (r1, r2, r3) = (
        rects.get("1001_1").copied(),
        rects.get("1001_2").copied(),
        rects.get("1001_3").copied(),
    );
    let (r1, r2, r3) = (
        r1.expect("win1 frame"),
        r2.expect("win2 frame"),
        r3.expect("win3 frame"),
    );
    // Outer margins on every area edge.
    assert_eq!(r1.0, OUTER, "left margin, got {r1:?}");
    assert_eq!(r1.1, OUTER, "top margin, got {r1:?}");
    assert_eq!(r1.3, 900 - 2 * OUTER, "full inset height, got {r1:?}");
    assert_eq!(r2.1, OUTER, "top margin, got {r2:?}");
    assert_eq!(r3.1, OUTER, "top margin, got {r3:?}");
    assert_eq!(r3.3, 900 - 2 * OUTER, "full inset height, got {r3:?}");
    assert_eq!(r3.0 + r3.2, 1440 - OUTER, "right margin, got {r3:?}");
    // Inner sibling spacing exactly one inner gap.
    assert_eq!(
        r2.0 - (r1.0 + r1.2),
        INNER,
        "win1|win2 gap, got {r1:?} {r2:?}"
    );
    assert_eq!(
        r3.0 - (r2.0 + r2.2),
        INNER,
        "win2|win3 gap, got {r2:?} {r3:?}"
    );
    // Columns tile the inset width without overlap.
    assert_eq!(
        r1.2 + INNER + r2.2 + INNER + r3.2,
        1440 - 2 * OUTER,
        "columns must exactly fill the inset width"
    );
    shutdown_clean(&fixture);
}

#[test]
fn native_focus_walk_retains_at_edge_per_tier1() {
    // Shared Tier-1 rule on the macOS path: repeated steps walk the row,
    // the exhausted edge retains (no wrap, no workspace cycling) counted
    // as `focus-rejected`, and later steps still dispatch after the miss.
    let fixture = start();
    let revision = fixture.handle.update_topology_with(one_display(), &|_| {
        Some(tiler_macos::ffi::SlsSpaceType::User)
    });
    install(
        &fixture,
        APP_A,
        scripted(
            vec![
                win_at(1, 8, 8, 466, 884),
                win_at(2, 482, 8, 466, 884),
                win_at(3, 956, 8, 468, 884),
            ],
            Some(1),
        ),
    );
    fixture.handle.update_focus_evidence(agreeing(APP_A, 1));
    fixture.handle.launch_app(APP_A).expect("launch");
    assert!(fixture.handle.flush(FLUSH));
    // Drain initial frames so later applied traffic is focus-driven.
    while fixture.applied_rx.try_recv().is_ok() {}

    // Step 1: win1 -> win2.
    send_focus_right(&fixture);
    let cmd = fixture.focus_rx.recv_timeout(WAIT).expect("first dispatch");
    assert_eq!(cmd.window, "1001_2");
    assert!(fixture.handle.flush(FLUSH));

    // Arrival on win2, step 2: win2 -> win3.
    let generation = fixture.handle.app_generation(APP_A).expect("generation");
    let windows = vec![
        win_at(1, 8, 8, 100, 100),
        win_at(2, 200, 8, 100, 100),
        win_at(3, 400, 8, 100, 100),
    ];
    assert_eq!(
        fixture.handle.submit_scoped_observation(arrival(
            APP_A,
            generation,
            50,
            windows.clone(),
            2,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    send_focus_right(&fixture);
    let cmd = fixture
        .focus_rx
        .recv_timeout(WAIT)
        .expect("second dispatch");
    assert_eq!(cmd.window, "1001_3");
    assert!(fixture.handle.flush(FLUSH));

    // Arrival on win3, step 3 at the exhausted edge: no dispatch.
    let generation = fixture.handle.app_generation(APP_A).expect("generation");
    assert_eq!(
        fixture.handle.submit_scoped_observation(arrival(
            APP_A,
            generation,
            51,
            windows.clone(),
            3,
            revision
        )),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    while fixture.focus_rx.try_recv().is_ok() {}
    send_focus_right(&fixture);
    assert!(
        fixture
            .focus_rx
            .recv_timeout(Duration::from_secs(2))
            .is_err(),
        "exhausted edge must retain without dispatch"
    );
    assert!(fixture.handle.flush(FLUSH));
    let diagnostics = fixture.handle.diagnostics();
    assert!(
        diagnostics
            .core_replies
            .get("focus-rejected-unchanged")
            .copied()
            .unwrap_or(0)
            >= 1,
        "edge retain must be counted: {:?}",
        diagnostics.core_replies
    );
    assert_eq!(
        diagnostics
            .core_replies
            .get("focus-accepted")
            .copied()
            .unwrap_or(0),
        2,
        "both walking steps must plan: {:?}",
        diagnostics.core_replies
    );

    // Recovery: an external arrival back on win2 steps to win3 again, so a
    // retained edge never wedges later focus.
    let generation = fixture.handle.app_generation(APP_A).expect("generation");
    assert_eq!(
        fixture
            .handle
            .submit_scoped_observation(arrival(APP_A, generation, 52, windows, 2, revision)),
        SubmitOutcome::Accepted
    );
    assert!(fixture.handle.flush(FLUSH));
    send_focus_right(&fixture);
    let cmd = fixture
        .focus_rx
        .recv_timeout(WAIT)
        .expect("recovery dispatch");
    assert_eq!(cmd.window, "1001_3");
    shutdown_clean(&fixture);
}

#[test]
fn focus_evidence_republish_is_change_only() {
    // Republishing identical evidence stays ring-silent; only genuine
    // changes emit. The controller pumps this every cadence, so an
    // unconditional emit is idle trace spam.
    let fixture = start();
    fixture.handle.update_topology_with(one_display(), &|_| {
        Some(tiler_macos::ffi::SlsSpaceType::User)
    });
    let evidence = agreeing(APP_A, 1);
    fixture.handle.update_focus_evidence(evidence.clone());
    fixture.handle.update_focus_evidence(evidence);
    fixture.handle.update_focus_evidence(agreeing(APP_A, 2));
    let count = fixture
        .handle
        .log_snapshot()
        .iter()
        .filter(|e| e.kind == "focus" && e.outcome == "evidence-updated")
        .count();
    assert_eq!(count, 2, "identical republish must stay silent");
    shutdown_clean(&fixture);
}
